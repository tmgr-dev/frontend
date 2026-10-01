import { parseJson } from '../serialize';
import { LocalHttpError, type LocalContext } from '../types';
import { conflict, forbidden, notFound, pageNotFound, tooLarge, unprocessable } from './errors';
import { emitPageEvent, type PageEventType } from './events';
import {
	AGENTS_OWNER,
	CONTEXT,
	CONTEXT_TITLE,
	PERSON,
	appendInSection,
	appendNewHeading,
	appendToEnd,
	appendUnderHeading,
	autolinkTaskKeys,
	containsSectionMarker,
	exceedsLimit,
	extractLinks,
	findSection,
	isKnownType,
	nonHumanViolation,
	replaceSection,
	sections,
	structureError,
	taskKeyCandidates,
	templateBody,
	writableBy,
} from './markdown';
import { InvalidProperties, LAST_CONTACT_AT, isChronicleHeading, prepareProperties } from './properties';
import {
	SUMMARY_COLUMNS,
	authorsFor,
	fileJson,
	pageJson,
	propertiesOf,
	snapshotJson,
	summaryJson,
	trashJson,
	versionJson,
	type FileRow,
	type PageRow,
	type VersionRow,
} from './serialize';
import { slugify, withSuffix } from './slug';
import { exclusive, inTransaction } from './tx';

const MAX_TITLE = 255;
const MAX_SUMMARY = 255;
const MAX_ATTEMPTS = 3;
const MAX_SLUG_ATTEMPTS = 6;
const DEFAULT_SEARCH_LIMIT = 20;
const MAX_SEARCH_LIMIT = 50;
const MAX_SEARCH_TERMS = 10;
const MAX_LINKS = 1000;
const CONTEXT_SEEDED = 'pages_context_seeded';

const iso = (ctx: LocalContext) => ctx.now().toISOString();

type ActorKind = 'user' | 'persona' | 'plugin';

const actorKind = (ctx: LocalContext): ActorKind =>
	ctx.actor?.kind === 'persona' ? 'persona' : ctx.actor?.kind === 'plugin' ? 'plugin' : 'user';

const actorRef = (ctx: LocalContext): string => (actorKind(ctx) === 'user' ? String(ctx.user.id) : ctx.actor!.id);

const isHuman = (ctx: LocalContext) => actorKind(ctx) === 'user';

const humanOnly = (ctx: LocalContext) => {
	if (!isHuman(ctx)) throw forbidden('forbidden', 'Only people can do this on a page');
};

class WriteRace extends Error {}

// ───────────────────────────────────────────────────────────── helpers

const canonical = (value: unknown): string =>
	JSON.stringify(value, (_, v) =>
		v && typeof v === 'object' && !Array.isArray(v)
			? Object.fromEntries(Object.entries(v as Record<string, unknown>).sort(([a], [b]) => (a < b ? -1 : 1)))
			: v,
	);

const sameJson = (a: string, b: string) => canonical(parseJson(a, null)) === canonical(parseJson(b, null));

const requireTitle = (title: unknown): string => {
	if (typeof title !== 'string' || !title.trim()) throw unprocessable('title_required', 'title is required');
	const t = title.trim();
	if (t.length > MAX_TITLE) throw unprocessable('title_too_long', `title must be at most ${MAX_TITLE} characters`);
	return t;
};

const cleanSummary = (raw: unknown): string | null => {
	if (typeof raw !== 'string' || !raw.trim()) return null;
	const s = raw.trim();
	return s.length > MAX_SUMMARY ? s.substring(0, MAX_SUMMARY) : s;
};

const blankToNull = (s: unknown): string | null => (typeof s === 'string' && s.trim() ? s.trim() : null);

const propertiesJson = (type: string, raw: unknown, existing: Record<string, any> | null): string => {
	try {
		return JSON.stringify(prepareProperties(type, raw, existing));
	} catch (error) {
		if (error instanceof InvalidProperties) {
			throw unprocessable('invalid_properties', error.message, { errors: error.errors });
		}
		throw error;
	}
};

export const taskKeyOf = (title: string | null, code: string | null, sequence: number | null, id: number): string => {
	const prefix = /^\s*([A-Za-z][A-Za-z0-9]*-\d+)\s*:/.exec(title ?? '');
	if (prefix) return prefix[1].toUpperCase();
	if (code && code.trim() && sequence != null) return `${code}-${sequence}`;
	return `T${id}`;
};

// ───────────────────────────────────────────────────────────── reads

const loadLive = async (ctx: LocalContext, id: number): Promise<PageRow | null> =>
	(await ctx.db.select<PageRow>(`SELECT * FROM pages WHERE id = ? AND deleted_at IS NULL`, [id]))[0] ?? null;

const lookup = async (ctx: LocalContext, ref: string, includeDeleted: boolean): Promise<PageRow | null> => {
	const trimmed = (ref ?? '').trim();
	if (!trimmed) return null;
	const where = includeDeleted ? '' : ' AND deleted_at IS NULL';
	if (/^\d+$/.test(trimmed)) {
		const id = Number(trimmed);
		if (!Number.isSafeInteger(id)) return null;
		return (await ctx.db.select<PageRow>(`SELECT * FROM pages WHERE id = ?${where}`, [id]))[0] ?? null;
	}
	return (await ctx.db.select<PageRow>(`SELECT * FROM pages WHERE slug = ?${where}`, [trimmed.toLowerCase()]))[0] ?? null;
};

const resolve = async (ctx: LocalContext, ref: string): Promise<PageRow> => {
	const row = await lookup(ctx, ref, false);
	if (!row) throw pageNotFound();
	return row;
};

const backlinksOf = (ctx: LocalContext, kind: string, targetId: number): Promise<PageRow[]> =>
	ctx.db.select<PageRow>(
		`SELECT p.* FROM page_links l JOIN pages p ON p.id = l.page_id
		 WHERE l.target_kind = ? AND l.target_id = ? AND p.deleted_at IS NULL
		 ORDER BY p.updated_at DESC, p.id DESC`,
		[kind, targetId],
	);

const filesOf = (ctx: LocalContext, pageId: number): Promise<FileRow[]> =>
	ctx.db.select<FileRow>(`SELECT * FROM files WHERE page_id = ? ORDER BY id`, [pageId]);

const assemble = async (ctx: LocalContext, row: PageRow) => {
	const names = await authorsFor(ctx, [
		{ kind: row.author_kind, ref: row.author_ref },
		{ kind: row.updated_by_kind, ref: row.updated_by_ref },
	]);
	return pageJson(row, ctx, names, await backlinksOf(ctx, 'page', row.id), await filesOf(ctx, row.id));
};

export const getPage = async (ctx: LocalContext, ref: string) => assemble(ctx, await resolve(ctx, ref));

export const listPages = async (ctx: LocalContext, parentId: number | null, type: string | null) => {
	const rows = await ctx.db.select<PageRow>(
		`SELECT ${SUMMARY_COLUMNS} FROM pages WHERE deleted_at IS NULL AND parent_id IS ?${type ? ' AND type = ?' : ''}
		 ORDER BY pinned DESC, position ASC, id ASC`,
		type ? [parentId, type] : [parentId],
	);
	return rows.map(summaryJson);
};

export const treePages = async (ctx: LocalContext) =>
	(
		await ctx.db.select<PageRow>(
			`SELECT ${SUMMARY_COLUMNS} FROM pages WHERE deleted_at IS NULL
			 ORDER BY COALESCE(parent_id, 0), pinned DESC, position ASC, id ASC`,
		)
	).map(summaryJson);

export const trashPages = async (ctx: LocalContext) =>
	(
		await ctx.db.select<PageRow>(
			`SELECT ${SUMMARY_COLUMNS} FROM pages p WHERE p.deleted_at IS NOT NULL
			 AND NOT EXISTS (SELECT 1 FROM pages q WHERE q.id = p.parent_id AND q.deleted_at = p.deleted_at)
			 ORDER BY p.deleted_at DESC, p.id DESC`,
		)
	).map(trashJson);

export const pageBacklinks = async (ctx: LocalContext, ref: string) =>
	(await backlinksOf(ctx, 'page', (await resolve(ctx, ref)).id)).map(summaryJson);

export const pagesForTask = async (ctx: LocalContext, taskId: number) => {
	const [task] = await ctx.db.select(`SELECT 1 FROM tasks WHERE id = ? AND deleted_at IS NULL`, [taskId]);
	if (!task) throw notFound('task_not_found', 'Task not found in this workspace');
	return (await backlinksOf(ctx, 'task', taskId)).map(summaryJson);
};

export const pageVersions = async (ctx: LocalContext, ref: string) => {
	const page = await resolve(ctx, ref);
	const rows = await ctx.db.select<VersionRow>(
		`SELECT page_id, version, title, '' AS body, '{}' AS properties, author_id, author_kind, author_ref, summary, created_at
		 FROM page_versions WHERE page_id = ? ORDER BY version DESC`,
		[page.id],
	);
	const names = await authorsFor(
		ctx,
		rows.map((r) => ({ kind: r.author_kind, ref: r.author_ref })),
	);
	return rows.map((r) => versionJson(r, ctx, names));
};

const loadVersion = async (ctx: LocalContext, pageId: number, version: number): Promise<VersionRow | null> =>
	(
		await ctx.db.select<VersionRow>(`SELECT * FROM page_versions WHERE page_id = ? AND version = ?`, [
			pageId,
			version,
		])
	)[0] ?? null;

export const pageVersion = async (ctx: LocalContext, ref: string, version: number) => {
	const page = await resolve(ctx, ref);
	const row = await loadVersion(ctx, page.id, version);
	if (!row) throw notFound('version_not_found', 'Version not found');
	const names = await authorsFor(ctx, [{ kind: row.author_kind, ref: row.author_ref }]);
	return snapshotJson(row, ctx, names);
};

export const workspaceContext = async (ctx: LocalContext): Promise<string> => {
	const rows = await ctx.db.select<PageRow>(
		`SELECT * FROM pages WHERE type = 'context' AND deleted_at IS NULL ORDER BY pinned DESC, position ASC, id ASC`,
	);
	return rows
		.map((row) => `# ${row.title}\n\n*Page \`${row.slug}\` (id ${row.id}, version ${row.version})*\n\n${row.body.trim()}`)
		.join('\n\n---\n\n');
};

// ───────────────────────────────────────────────────────────── search

export const searchTerms = (query: unknown): string[] => {
	if (typeof query !== 'string') return [];
	const terms: string[] = [];
	for (const token of query.split(/[^\p{L}\p{N}]+/u)) {
		if (token.length >= 2 && terms.length < MAX_SEARCH_TERMS) terms.push(token);
	}
	return terms;
};

const stripComments = (body: string): string => {
	let out = '';
	let last = 0;
	for (let i = body.indexOf('<!--', last); i >= 0; i = body.indexOf('<!--', last)) {
		const end = body.indexOf('-->', i + 4);
		if (end < 0) break;
		out += `${body.slice(last, i)} `;
		last = end + 3;
	}
	return out + body.slice(last);
};

export const snippet = (body: string, terms: string[]): string => {
	const text = stripComments(body).replace(/\s+/g, ' ').trim();
	const lower = text.toLowerCase();
	let at = -1;
	for (const term of terms) {
		const found = lower.indexOf(term.toLowerCase());
		if (found >= 0 && (at < 0 || found < at)) at = found;
	}
	if (at < 0) return text.length <= 160 ? text : `${text.substring(0, 160)}…`;
	const from = Math.max(0, at - 60);
	const to = Math.min(text.length, at + 100);
	return `${from > 0 ? '…' : ''}${text.substring(from, to)}${to < text.length ? '…' : ''}`;
};

export const searchPages = async (ctx: LocalContext, query: unknown, type: unknown, limit: unknown) => {
	const terms = searchTerms(query);
	if (!terms.length) return [];
	const requested = Number(limit);
	const bounded = Number.isFinite(requested) && requested > 0 ? Math.min(Math.floor(requested), MAX_SEARCH_LIMIT) : DEFAULT_SEARCH_LIMIT;
	const match = terms.map((t) => `"${t}"*`).join(' ');
	const kind = blankToNull(type);
	let rows: PageRow[];
	try {
		rows = await ctx.db.select<PageRow>(
			`SELECT p.* FROM pages_fts JOIN pages p ON p.id = pages_fts.rowid
			 WHERE pages_fts MATCH ? AND p.deleted_at IS NULL${kind ? ' AND p.type = ?' : ''}
			 ORDER BY pages_fts.rank, p.updated_at DESC LIMIT ?`,
			kind ? [match, kind, bounded] : [match, bounded],
		);
	} catch {
		return [];
	}
	return rows.map((r) => ({
		id: r.id,
		slug: r.slug,
		title: r.title,
		type: r.type,
		snippet: snippet(r.body, terms),
		updated_at: r.updated_at,
	}));
};

// ───────────────────────────────────────────────────────────── links

const existingTargets = async (
	ctx: LocalContext,
	selfPageId: number,
	candidates: { kind: string; id: number }[],
): Promise<Set<string>> => {
	const byKind = new Map<string, number[]>();
	for (const ref of candidates.slice(0, MAX_LINKS)) {
		if (ref.kind === 'page' && ref.id === selfPageId) continue;
		if (ref.kind === 'persona') continue;
		byKind.set(ref.kind, [...(byKind.get(ref.kind) ?? []), ref.id]);
	}
	const found = new Set<string>();
	const tables: Record<string, string> = {
		page: 'pages',
		task: 'tasks',
		category: 'categories',
	};
	for (const [kind, ids] of byKind) {
		if (kind === 'user') {
			if (ids.includes(ctx.user.id)) found.add(`user:${ctx.user.id}`);
			continue;
		}
		const rows = await ctx.db.select<{ id: number }>(
			`SELECT id FROM ${tables[kind]} WHERE deleted_at IS NULL AND id IN (${ids.map(() => '?').join(',')})`,
			ids,
		);
		rows.forEach((r) => found.add(`${kind}:${r.id}`));
	}
	return found;
};

/** Rewrites page_links from the body; returns the task ids whose mention was added or removed. */
const syncLinks = async (ctx: LocalContext, page: PageRow, created: boolean): Promise<number[]> => {
	const wanted = await existingTargets(ctx, page.id, extractLinks(page.body));
	const stored = created
		? []
		: await ctx.db.select<{ target_kind: string; target_id: number }>(
				`SELECT target_kind, target_id FROM page_links WHERE page_id = ?`,
				[page.id],
		  );
	const before = new Set(stored.map((r) => `${r.target_kind}:${r.target_id}`));
	const tasks: number[] = [];
	for (const key of before) {
		if (wanted.has(key)) continue;
		const [kind, id] = key.split(':');
		await ctx.db.execute(`DELETE FROM page_links WHERE page_id = ? AND target_kind = ? AND target_id = ?`, [
			page.id,
			kind,
			Number(id),
		]);
		if (kind === 'task') tasks.push(Number(id));
	}
	for (const key of wanted) {
		if (before.has(key)) continue;
		const [kind, id] = key.split(':');
		await ctx.db.execute(`INSERT OR IGNORE INTO page_links (page_id, target_kind, target_id) VALUES (?, ?, ?)`, [
			page.id,
			kind,
			Number(id),
		]);
		if (kind === 'task') tasks.push(Number(id));
	}
	return tasks;
};

const resolveTaskKeys = async (ctx: LocalContext, keys: string[]): Promise<Map<string, number>> => {
	const out = new Map<string, number>();
	for (const key of keys) {
		const dash = key.lastIndexOf('-');
		const code = key.substring(0, dash);
		const sequence = Number(key.substring(dash + 1));
		if (!Number.isSafeInteger(sequence)) continue;
		const rows = await ctx.db.select<any>(
			`SELECT t.id, t.title, c.code, t.category_tasks_sequence_id AS seq FROM tasks t
			 LEFT JOIN categories c ON c.id = t.project_category_id
			 WHERE t.deleted_at IS NULL AND (t.title LIKE ? OR (c.code = ? AND t.category_tasks_sequence_id = ?)) LIMIT 20`,
			[`${key}:%`, code, sequence],
		);
		const hit = rows.find((r) => taskKeyOf(r.title, r.code, r.seq, r.id).toLowerCase() === key.toLowerCase());
		if (hit) out.set(key, hit.id);
	}
	return out;
};

const finalizeBody = async (ctx: LocalContext, body: string): Promise<string> => {
	const keys = taskKeyCandidates(body);
	const linked = keys.length ? autolinkTaskKeys(body, await resolveTaskKeys(ctx, keys)) : body;
	if (exceedsLimit(linked)) throw tooLarge();
	return linked;
};

const requireWellFormed = (body: string) => {
	const why = structureError(body);
	if (why) throw unprocessable('invalid_sections', why);
};

const requireWritable = (
	ctx: LocalContext,
	oldBody: string,
	newBody: string,
	contextPage: boolean,
	oldTitle: string,
	newTitle: string,
	oldProps: string,
	newProps: string,
) => {
	if (isHuman(ctx)) return;
	if (contextPage && (oldTitle !== newTitle || !sameJson(oldProps, newProps))) {
		throw forbidden('section_forbidden', "A context page's title and properties are for people");
	}
	const why = nonHumanViolation(oldBody, newBody, contextPage, actorKind(ctx), actorRef(ctx));
	if (why) throw forbidden('section_forbidden', why);
};

// ───────────────────────────────────────────────────────────── write core

interface Change {
	title: string;
	body: string;
	properties: string;
	summary: string | null;
	forceVersion?: boolean;
}

interface Written {
	row: PageRow;
	linked: number[];
	changed: boolean;
	summary: string | null;
}

const persist = async (
	ctx: LocalContext,
	row: PageRow,
	change: Change,
	finalBody: string,
	bodyChanged: boolean,
): Promise<Written> => {
	const kind = actorKind(ctx);
	const ref = actorRef(ctx);
	const result = await ctx.db.execute(
		`UPDATE pages SET title = ?, body = ?, properties = ?, version = version + 1,
			updated_by_id = ?, updated_by_kind = ?, updated_by_ref = ?, updated_at = ?
		 WHERE id = ? AND version = ? AND deleted_at IS NULL`,
		[change.title, finalBody, change.properties, ctx.user.id, kind, ref, iso(ctx), row.id, row.version],
	);
	if (!result.rowsAffected) throw new WriteRace();
	const [updated] = await ctx.db.select<PageRow>(`SELECT * FROM pages WHERE id = ?`, [row.id]);
	await ctx.db.execute(
		`INSERT INTO page_versions (page_id, version, title, body, properties, author_id, author_kind, author_ref, summary, created_at)
		 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)`,
		[updated.id, updated.version, updated.title, updated.body, updated.properties, ctx.user.id, kind, ref, change.summary, iso(ctx)],
	);
	const linked = bodyChanged ? await syncLinks(ctx, updated, false) : [];
	return { row: updated, linked, changed: true, summary: change.summary };
};

const write = async (
	ctx: LocalContext,
	pageId: number,
	plan: (row: PageRow) => Promise<Change> | Change,
	eventType: PageEventType = 'page.updated',
) => {
	const written = await exclusive(ctx.db, async (): Promise<Written> => {
		for (let attempt = 0; attempt < MAX_ATTEMPTS; attempt++) {
			const row = await loadLive(ctx, pageId);
			if (!row) throw pageNotFound();
			const change = await plan(row);
			if (exceedsLimit(change.body)) throw tooLarge();
			const bodyChanged = change.body !== row.body;
			requireWritable(
				ctx,
				row.body,
				change.body,
				row.type === CONTEXT,
				row.title,
				change.title,
				row.properties,
				change.properties,
			);
			if (bodyChanged) requireWellFormed(change.body);
			const finalBody = bodyChanged ? await finalizeBody(ctx, change.body) : change.body;
			if (!change.forceVersion && !bodyChanged && change.title === row.title && sameJson(change.properties, row.properties)) {
				return { row, linked: [], changed: false, summary: change.summary };
			}
			try {
				return await inTransaction(ctx.db, () => persist(ctx, row, change, finalBody, bodyChanged));
			} catch (error) {
				if (!(error instanceof WriteRace)) throw error;
			}
		}
		const current = await loadLive(ctx, pageId);
		if (!current) throw pageNotFound();
		throw conflict(await assemble(ctx, current));
	});
	if (written.changed) emitPageEvent(ctx, eventType, written.row, written.summary, written.linked);
	return assemble(ctx, written.row);
};

// ───────────────────────────────────────────────────────────── create

interface NewPage {
	title: string;
	type: string;
	parentId: number | null;
	body: string;
	properties: string;
	pinned: boolean;
	fromTemplate: boolean;
}

const nextPosition = async (ctx: LocalContext, parentId: number | null): Promise<number> => {
	const [row] = await ctx.db.select<{ next: number }>(
		`SELECT COALESCE(MAX(position) + 1, 0) AS next FROM pages WHERE deleted_at IS NULL AND parent_id IS ?`,
		[parentId],
	);
	return Number(row?.next ?? 0);
};

const slugTaken = async (ctx: LocalContext, slug: string): Promise<boolean> =>
	(await ctx.db.select(`SELECT 1 FROM pages WHERE slug = ? LIMIT 1`, [slug])).length > 0;

const insertRow = async (ctx: LocalContext, np: NewPage, body: string, position: number): Promise<number> => {
	const base = slugify(np.title);
	let slug = (await slugTaken(ctx, base)) ? withSuffix(base) : base;
	for (let attempt = 0; ; attempt++) {
		try {
			const kind = actorKind(ctx);
			const ref = actorRef(ctx);
			const at = iso(ctx);
			const result = await ctx.db.execute(
				`INSERT INTO pages (parent_id, title, slug, type, body, properties, version, author_id, author_kind, author_ref,
					updated_by_id, updated_by_kind, updated_by_ref, position, pinned, created_at, updated_at)
				 VALUES (?, ?, ?, ?, ?, ?, 1, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)`,
				[np.parentId, np.title, slug, np.type, body, np.properties, ctx.user.id, kind, ref, ctx.user.id, kind, ref, position, np.pinned ? 1 : 0, at, at],
			);
			return Number(result.lastInsertId);
		} catch (error) {
			const message = error instanceof Error ? error.message : String(error);
			if (!/UNIQUE/i.test(message) || attempt + 1 >= MAX_SLUG_ATTEMPTS) throw error;
			slug = withSuffix(base);
		}
	}
};

const insertPage = async (ctx: LocalContext, np: NewPage): Promise<{ row: PageRow; linked: number[] }> =>
	exclusive(ctx.db, async () => {
		if (np.parentId !== null && !(await loadLive(ctx, np.parentId))) {
			throw unprocessable('parent_not_found', 'Parent page not found');
		}
		if (exceedsLimit(np.body)) throw tooLarge();
		if (!np.fromTemplate) requireWritable(ctx, '', np.body, false, np.title, np.title, np.properties, np.properties);
		requireWellFormed(np.body);
		const body = await finalizeBody(ctx, np.body);
		return inTransaction(ctx.db, async () => {
			const id = await insertRow(ctx, np, body, await nextPosition(ctx, np.parentId));
			const [row] = await ctx.db.select<PageRow>(`SELECT * FROM pages WHERE id = ?`, [id]);
			await ctx.db.execute(
				`INSERT INTO page_versions (page_id, version, title, body, properties, author_id, author_kind, author_ref, summary, created_at)
				 VALUES (?, 1, ?, ?, ?, ?, ?, ?, NULL, ?)`,
				[row.id, row.title, row.body, row.properties, ctx.user.id, actorKind(ctx), actorRef(ctx), iso(ctx)],
			);
			return { row, linked: await syncLinks(ctx, row, true) };
		});
	});

export const createPage = async (ctx: LocalContext, req: any) => {
	const title = requireTitle(req?.title);
	const type = typeof req?.type === 'string' && req.type.trim() ? req.type.trim() : 'plain';
	if (!isKnownType(type)) throw unprocessable('unsupported_type', `Page type '${type}' is not supported`);
	if (type === CONTEXT && !isHuman(ctx)) throw forbidden('forbidden', 'Only people can create context pages');
	const parentId = req?.parent_id === null || req?.parent_id === undefined ? null : Number(req.parent_id);
	if (parentId !== null && !Number.isSafeInteger(parentId)) throw unprocessable('parent_not_found', 'Parent page not found');
	const properties = propertiesJson(type, req?.properties, null);
	const fromTemplate = req?.body === undefined || req?.body === null;
	if (!fromTemplate && typeof req.body !== 'string') throw unprocessable('invalid_body', 'body must be a string');
	const body: string = fromTemplate ? templateBody(type) : req.body;
	if (exceedsLimit(body)) throw tooLarge();
	const { row, linked } = await insertPage(ctx, {
		title,
		type,
		parentId,
		body,
		properties,
		pinned: type === CONTEXT,
		fromTemplate,
	});
	emitPageEvent(ctx, 'page.created', row, null, linked);
	return assemble(ctx, row);
};

/** Creates the pinned context page the first time a local workspace is opened; never again, so it can be deleted. */
export const ensureContextPage = async (ctx: LocalContext): Promise<void> => {
	const claimed = await ctx.db.execute(`INSERT OR IGNORE INTO meta (key, value) VALUES (?, '1')`, [CONTEXT_SEEDED]);
	if (!claimed.rowsAffected) return;
	try {
		const [existing] = await ctx.db.select(`SELECT 1 FROM pages WHERE type = 'context' LIMIT 1`);
		if (existing) return;
		await insertPage(ctx, {
			title: CONTEXT_TITLE,
			type: CONTEXT,
			parentId: null,
			body: templateBody(CONTEXT),
			properties: '{}',
			pinned: true,
			fromTemplate: true,
		});
	} catch (error) {
		await ctx.db.execute(`DELETE FROM meta WHERE key = ?`, [CONTEXT_SEEDED]);
		throw error;
	}
};

// ───────────────────────────────────────────────────────────── update / append / sections

export const updatePage = async (ctx: LocalContext, ref: string, req: any) => {
	if (req?.version === undefined || req?.version === null) throw unprocessable('version_required', 'version is required');
	const page = await resolve(ctx, ref);
	return write(ctx, page.id, async (row) => {
		if (row.version !== Number(req.version)) throw conflict(await assemble(ctx, row));
		const title = req.title === undefined || req.title === null ? row.title : requireTitle(req.title);
		if (req.body !== undefined && req.body !== null && typeof req.body !== 'string') {
			throw unprocessable('invalid_body', 'body must be a string');
		}
		const body: string = req.body ?? row.body;
		if (exceedsLimit(body)) throw tooLarge();
		const properties =
			req.properties === undefined || req.properties === null
				? row.properties
				: propertiesJson(row.type, req.properties, propertiesOf(row.properties));
		return { title, body, properties, summary: cleanSummary(req.summary) };
	});
};

const today = (ctx: LocalContext) => iso(ctx).slice(0, 10);

const appended = (ctx: LocalContext, row: PageRow, req: any): string => {
	const heading = typeof req.heading === 'string' && req.heading.trim() ? req.heading : null;
	if (heading) {
		const inserted = appendUnderHeading(row.body, heading, req.markdown);
		if (inserted !== null) return inserted;
		if (req.create_heading === true) return appendNewHeading(row.body, heading, req.markdown);
		throw unprocessable('heading_not_found', `Heading '${heading.trim()}' not found`);
	}
	if (!isHuman(ctx) && row.type === CONTEXT) {
		const agents = sections(row.body).find((s) => s.owner === AGENTS_OWNER);
		if (!agents) throw forbidden('section_forbidden', 'This context page has no section agents may write to');
		return appendInSection(row.body, agents, req.markdown);
	}
	return appendToEnd(row.body, req.markdown);
};

export const appendToPage = async (ctx: LocalContext, ref: string, req: any) => {
	if (typeof req?.markdown !== 'string' || !req.markdown.trim()) {
		throw unprocessable('markdown_required', 'markdown is required');
	}
	if (exceedsLimit(req.markdown)) throw tooLarge();
	if (!isHuman(ctx) && containsSectionMarker(req.markdown)) {
		throw forbidden('section_forbidden', 'Agents cannot write section markers');
	}
	const page = await resolve(ctx, ref);
	return write(ctx, page.id, (row) => {
		const body = appended(ctx, row, req);
		let properties = row.properties;
		if (row.type === PERSON && isChronicleHeading(req.heading)) {
			properties = JSON.stringify({ ...propertiesOf(row.properties), [LAST_CONTACT_AT]: today(ctx) });
		}
		return { title: row.title, body, properties, summary: cleanSummary(req.summary) };
	});
};

export const setPageSection = async (ctx: LocalContext, ref: string, sectionId: string, req: any) => {
	if (typeof req?.markdown !== 'string') throw unprocessable('markdown_required', 'markdown is required');
	if (exceedsLimit(req.markdown)) throw tooLarge();
	if (!isHuman(ctx) && containsSectionMarker(req.markdown)) {
		throw forbidden('section_forbidden', 'Agents cannot write section markers');
	}
	const page = await resolve(ctx, ref);
	return write(
		ctx,
		page.id,
		(row) => {
			const section = findSection(row.body, sectionId);
			if (!section) throw notFound('section_not_found', `Section '${sectionId}' not found`);
			if (!isHuman(ctx) && !writableBy(section.owner, actorKind(ctx), actorRef(ctx), row.type === CONTEXT)) {
				throw forbidden('section_forbidden', `Section '${sectionId}' is managed by ${section.owner}`);
			}
			return {
				title: row.title,
				body: replaceSection(row.body, section, req.markdown),
				properties: row.properties,
				summary: cleanSummary(req.summary),
			};
		},
		'page.updated',
	);
};

export const restorePageVersion = async (ctx: LocalContext, ref: string, version: number) => {
	humanOnly(ctx);
	const page = await resolve(ctx, ref);
	return write(ctx, page.id, async (row) => {
		const snapshot = await loadVersion(ctx, row.id, version);
		if (!snapshot) throw notFound('version_not_found', 'Version not found');
		return {
			title: snapshot.title,
			body: snapshot.body,
			properties: snapshot.properties,
			summary: `Restored version ${version}`,
			forceVersion: true,
		};
	});
};

/** The system «Обещания» section of a person page, rewritten as the human who caused it. */
export const refreshManagedSection = async (
	ctx: LocalContext,
	pageId: number,
	sectionId: string,
	markdown: string,
	summary: string,
): Promise<void> => {
	const asUser: LocalContext = { ...ctx, actor: undefined };
	try {
		await write(asUser, pageId, (row) => {
			const section = findSection(row.body, sectionId);
			const body = section ? replaceSection(row.body, section, markdown) : row.body;
			return { title: row.title, body, properties: row.properties, summary };
		});
	} catch (error) {
		if (!(error instanceof LocalHttpError)) throw error;
	}
};

/** Replaces the first occurrence of `text` in the latest version of the page, whatever version the caller saw. */
export const replaceFirst = async (ctx: LocalContext, pageId: number, text: string, replacement: string, summary: string) =>
	write(ctx, pageId, async (row) => {
		const at = row.body.indexOf(text);
		if (at < 0) throw conflict(await assemble(ctx, row));
		return {
			title: row.title,
			body: row.body.substring(0, at) + replacement + row.body.substring(at + text.length),
			properties: row.properties,
			summary: cleanSummary(summary),
		};
	});

// ───────────────────────────────────────────────────────────── tree operations

const isDescendantOrSelf = async (ctx: LocalContext, ancestorId: number, candidateId: number): Promise<boolean> =>
	(
		await ctx.db.select(
			`WITH RECURSIVE up (id, parent_id) AS (
				SELECT id, parent_id FROM pages WHERE id = ?
				UNION ALL SELECT p.id, p.parent_id FROM pages p JOIN up ON p.id = up.parent_id)
			 SELECT 1 FROM up WHERE id = ? LIMIT 1`,
			[candidateId, ancestorId],
		)
	).length > 0;

export const movePage = async (ctx: LocalContext, ref: string, req: any) => {
	humanOnly(ctx);
	const page = await resolve(ctx, ref);
	const parentId = req?.parent_id === null || req?.parent_id === undefined ? null : Number(req.parent_id);
	if (parentId !== null && !Number.isSafeInteger(parentId)) throw unprocessable('parent_not_found', 'Parent page not found');
	const moved = await exclusive(ctx.db, async () => {
		const row = await loadLive(ctx, page.id);
		if (!row) throw pageNotFound();
		if (parentId !== null) {
			if (parentId === row.id) throw unprocessable('page_cycle', 'A page cannot be moved under itself');
			if (!(await loadLive(ctx, parentId))) throw unprocessable('parent_not_found', 'Parent page not found');
			if (await isDescendantOrSelf(ctx, row.id, parentId)) {
				throw unprocessable('page_cycle', 'A page cannot be moved under its own descendant');
			}
		}
		const siblings = (
			await ctx.db.select<{ id: number }>(
				`SELECT id FROM pages WHERE deleted_at IS NULL AND id <> ? AND parent_id IS ? ORDER BY pinned DESC, position ASC, id ASC`,
				[row.id, parentId],
			)
		).map((r) => r.id);
		const requested = Number(req?.position);
		const at = Number.isFinite(requested) ? Math.max(0, Math.min(Math.floor(requested), siblings.length)) : siblings.length;
		siblings.splice(at, 0, row.id);
		return inTransaction(ctx.db, async () => {
			for (const [index, id] of siblings.entries()) {
				if (id === row.id) {
					await ctx.db.execute(`UPDATE pages SET parent_id = ?, position = ?, updated_at = ? WHERE id = ?`, [
						parentId,
						index,
						iso(ctx),
						id,
					]);
				} else {
					await ctx.db.execute(`UPDATE pages SET position = ? WHERE id = ? AND position <> ?`, [index, id, index]);
				}
			}
			return (await ctx.db.select<PageRow>(`SELECT * FROM pages WHERE id = ?`, [row.id]))[0];
		});
	});
	emitPageEvent(ctx, 'page.moved', moved, null, []);
	return assemble(ctx, moved);
};

export const pinPage = async (ctx: LocalContext, ref: string, pinned: boolean) => {
	humanOnly(ctx);
	const page = await resolve(ctx, ref);
	const row = await exclusive(ctx.db, async () => {
		if (!(await loadLive(ctx, page.id))) throw pageNotFound();
		await ctx.db.execute(`UPDATE pages SET pinned = ?, updated_at = ? WHERE id = ?`, [pinned ? 1 : 0, iso(ctx), page.id]);
		return (await ctx.db.select<PageRow>(`SELECT * FROM pages WHERE id = ?`, [page.id]))[0];
	});
	emitPageEvent(ctx, 'page.moved', row, null, []);
	return assemble(ctx, row);
};

export const deletePage = async (ctx: LocalContext, ref: string) => {
	humanOnly(ctx);
	const page = await resolve(ctx, ref);
	const { row, deleted } = await exclusive(ctx.db, async () => {
		const row = await loadLive(ctx, page.id);
		if (!row) throw pageNotFound();
		const subtree = await ctx.db.select<{ id: number }>(
			`WITH RECURSIVE sub (id) AS (
				SELECT id FROM pages WHERE id = ?
				UNION ALL SELECT p.id FROM pages p JOIN sub s ON p.parent_id = s.id WHERE p.deleted_at IS NULL)
			 SELECT id FROM sub`,
			[row.id],
		);
		const ids = subtree.map((r) => r.id);
		const result = await ctx.db.execute(
			`UPDATE pages SET deleted_at = ? WHERE id IN (${ids.map(() => '?').join(',')}) AND deleted_at IS NULL`,
			[iso(ctx), ...ids],
		);
		return { row, deleted: result.rowsAffected };
	});
	emitPageEvent(ctx, 'page.deleted', row, null, []);
	return { deleted };
};

export const restorePage = async (ctx: LocalContext, ref: string) => {
	humanOnly(ctx);
	const page = await lookup(ctx, ref, true);
	if (!page) throw pageNotFound();
	const restored = await exclusive(ctx.db, async () => {
		const [row] = await ctx.db.select<PageRow>(`SELECT * FROM pages WHERE id = ?`, [page.id]);
		if (!row) throw pageNotFound();
		if (row.deleted_at === null) throw unprocessable('page_not_deleted', 'Page is not deleted');
		const subtree = await ctx.db.select<{ id: number }>(
			`WITH RECURSIVE sub (id) AS (
				SELECT id FROM pages WHERE id = ?
				UNION ALL SELECT p.id FROM pages p JOIN sub s ON p.parent_id = s.id WHERE p.deleted_at = ?)
			 SELECT id FROM sub`,
			[row.id, row.deleted_at],
		);
		const ids = subtree.map((r) => r.id);
		await ctx.db.execute(
			`UPDATE pages SET deleted_at = NULL, updated_at = ? WHERE id IN (${ids.map(() => '?').join(',')})`,
			[iso(ctx), ...ids],
		);
		if (row.parent_id !== null && !(await loadLive(ctx, row.parent_id))) {
			await ctx.db.execute(`UPDATE pages SET parent_id = NULL, position = ? WHERE id = ?`, [
				await nextPosition(ctx, null),
				row.id,
			]);
		}
		return (await ctx.db.select<PageRow>(`SELECT * FROM pages WHERE id = ?`, [row.id]))[0];
	});
	emitPageEvent(ctx, 'page.restored', restored, null, []);
	return assemble(ctx, restored);
};

// ───────────────────────────────────────────────────────────── files

const FILE_KEY = /^[A-Za-z0-9-]+\/[A-Za-z0-9._-]+$/;

export const pageFiles = async (ctx: LocalContext, ref: string) => {
	const page = await resolve(ctx, ref);
	return (await filesOf(ctx, page.id)).map((f) => fileJson(f, ctx));
};

export const attachPageFile = async (ctx: LocalContext, ref: string, req: any) => {
	const page = await resolve(ctx, ref);
	const byId = req?.file_id !== undefined && req?.file_id !== null;
	const byPath = typeof req?.file_path === 'string' && req.file_path.trim() !== '';
	if (byId === byPath) {
		throw unprocessable('invalid_file', 'Send either file_id or file_name, file_path, mime_type and size_bytes');
	}
	if (byId) {
		const [file] = await ctx.db.select<any>(`SELECT * FROM files WHERE id = ?`, [Number(req.file_id)]);
		if (!file) throw notFound('file_not_found', 'File not found');
		if (file.task_id !== null) throw unprocessable('file_bound_to_task', 'The file is attached to a task');
		if (file.page_id !== null && file.page_id !== page.id) {
			throw unprocessable('file_bound_to_page', 'The file is attached to another page');
		}
		await ctx.db.execute(`UPDATE files SET page_id = ? WHERE id = ?`, [page.id, file.id]);
		return fileJson({ ...file, page_id: page.id }, ctx);
	}
	if (typeof req.file_name !== 'string' || !req.file_name.trim()) {
		throw unprocessable('invalid_file', 'file_name is required');
	}
	if (!FILE_KEY.test(req.file_path)) throw unprocessable('invalid_file_path', 'Unknown file');
	const [existing] = await ctx.db.select<any>(`SELECT * FROM files WHERE file_path = ?`, [req.file_path]);
	if (existing) {
		if (existing.page_id !== page.id) throw unprocessable('invalid_file_path', 'The file is already attached elsewhere');
		return fileJson(existing, ctx);
	}
	const result = await ctx.db.execute(
		`INSERT INTO files (task_id, page_id, name, file_path, mime_type, size, created_at) VALUES (NULL, ?, ?, ?, ?, ?, ?)`,
		[page.id, req.file_name.trim(), req.file_path, req.mime_type ?? null, Number(req.size_bytes ?? 0), iso(ctx)],
	);
	const [row] = await ctx.db.select<any>(`SELECT * FROM files WHERE id = ?`, [Number(result.lastInsertId)]);
	return fileJson(row, ctx);
};

// ───────────────────────────────────────────────────────────── task from selection

const MAX_TASK_TITLE = 200;

export const taskTitleOf = (text: string): string => {
	const first = (text.trim().split(/\r\n|\r|\n/)[0] ?? '').trim();
	return first.length <= MAX_TASK_TITLE ? first : `${first.substring(0, MAX_TASK_TITLE).trim()}…`;
};

export const taskFromSelection = async (ctx: LocalContext, ref: string, req: any) => {
	if (req?.version === undefined || req?.version === null) throw unprocessable('version_required', 'version is required');
	if (typeof req?.text !== 'string' || !req.text.trim()) throw unprocessable('text_required', 'text is required');
	if (req?.category_id === undefined || req?.category_id === null) {
		throw unprocessable('category_required', 'category_id is required');
	}
	const page = await resolve(ctx, ref);
	if (page.version !== Number(req.version)) throw conflict(await assemble(ctx, page));
	if (!page.body.includes(req.text)) throw unprocessable('selection_not_found', 'The selected text is not in the page');
	const categoryId = Number(req.category_id);
	const [category] = await ctx.db.select<any>(`SELECT id, code FROM categories WHERE id = ? AND deleted_at IS NULL`, [categoryId]);
	if (!category) throw notFound('category_not_found', 'Category not found in this workspace');
	let statusId: number | null = null;
	if (req.status_id !== undefined && req.status_id !== null) {
		const [status] = await ctx.db.select<any>(`SELECT id FROM statuses WHERE id = ?`, [Number(req.status_id)]);
		if (!status) throw notFound('status_not_found', 'Status not found in this workspace');
		statusId = status.id;
	} else {
		const [fallback] = await ctx.db.select<any>(
			`SELECT id FROM statuses ORDER BY (type = 'default') DESC, (type = 'active') DESC, sort_order ASC, id ASC LIMIT 1`,
		);
		statusId = fallback?.id ?? null;
	}
	const text: string = req.text;
	const now = iso(ctx);
	const created = await ctx.db.execute(
		`INSERT INTO tasks (title, description, status_id, project_category_id, category_tasks_sequence_id, created_at, updated_at)
		 VALUES (?, ?, ?, ?, (SELECT COALESCE(MAX(category_tasks_sequence_id), 0) + 1 FROM tasks WHERE project_category_id = ?), ?, ?)`,
		[taskTitleOf(text), `${text.trim()}\n\nИз страницы: [${page.title}](tmgr://page/${page.id})`, statusId, categoryId, categoryId, now, now],
	);
	const taskId = Number(created.lastInsertId);
	const [task] = await ctx.db.select<any>(`SELECT id, title, category_tasks_sequence_id AS seq FROM tasks WHERE id = ?`, [taskId]);
	const key = taskKeyOf(task.title, category.code, task.seq, taskId);
	let updated;
	try {
		updated = await replaceFirst(ctx, page.id, text, `[${key}](tmgr://task/${taskId})`, `Создана задача ${key}`);
	} catch (error) {
		await ctx.db.execute(`UPDATE tasks SET deleted_at = ?, updated_at = ? WHERE id = ?`, [now, now, taskId]);
		throw error;
	}
	return { taskId, key, page: updated };
};
