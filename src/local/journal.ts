import type { BatchResult, BatchStatement, LocalContext, LocalDb } from './types';

export const GENESIS_HASH = '0'.repeat(64);

export type AuthorKind = 'human' | 'agent' | 'plugin' | 'unattended';
export type JournalSource =
	| 'desktop_ui'
	| 'cli_tty'
	| 'cli'
	| 'local_access'
	| 'mcp'
	| 'runner'
	| 'plugin'
	| 'import';
export type JournalField = 'create' | 'title' | 'description' | 'comment' | 'delete';

export interface JournalRow {
	seq: number;
	at: string;
	entity: 'task' | 'comment';
	entity_id: string;
	task_id: string;
	field: JournalField;
	author_kind: AuthorKind;
	source: JournalSource;
	actor_id: string;
	content_sha256: string;
	prev_hash: string;
	hash: string;
}

export interface JournalEntry {
	entity: 'task' | 'comment';
	entityId: number;
	taskId: number;
	field: JournalField;
	/** The field's new value; `null` for a delete. */
	content: string | null;
}

export interface Author {
	author_kind: AuthorKind;
	source: JournalSource;
	actor_id: string;
}

const MAX_ATTEMPTS = 12;

const hex = (buffer: ArrayBuffer): string =>
	Array.from(new Uint8Array(buffer), (byte) => byte.toString(16).padStart(2, '0')).join('');

export const sha256Hex = async (text: string): Promise<string> =>
	hex(await crypto.subtle.digest('SHA-256', new TextEncoder().encode(text)));

export const contentSha = (value: string | null | undefined): Promise<string> =>
	sha256Hex(String(value ?? '').replace(/\r\n/g, '\n'));

/**
 * The canonical row encoding: a JSON array of the eleven fields below in this order, UTF-8, no
 * whitespace, hashed with SHA-256 and written as lowercase hex. `seq` is a JSON number, the rest
 * are strings.
 */
export const rowHash = (row: Omit<JournalRow, 'hash'>): Promise<string> =>
	sha256Hex(
		JSON.stringify([
			row.seq,
			row.at,
			row.entity,
			row.entity_id,
			row.task_id,
			row.field,
			row.author_kind,
			row.source,
			row.actor_id,
			row.content_sha256,
			row.prev_hash,
		]),
	);

/** Who the journal blames for a write. Anything that is not the app's own client is never human. */
export const authorOf = (ctx: LocalContext): Author => {
	const actor = ctx.actor;
	if (!actor) return { author_kind: 'human', source: 'desktop_ui', actor_id: String(ctx.user.id) };
	switch (actor.kind) {
		case 'persona':
			return { author_kind: 'agent', source: actor.via === 'mcp' ? 'mcp' : 'local_access', actor_id: actor.id };
		case 'agent':
			return { author_kind: 'agent', source: 'runner', actor_id: actor.id };
		case 'companion':
			return { author_kind: 'agent', source: 'local_access', actor_id: actor.id };
		case 'plugin':
			return { author_kind: 'plugin', source: 'plugin', actor_id: actor.ownerId ?? actor.id };
		default:
			return { author_kind: 'unattended', source: 'import', actor_id: actor.id };
	}
};

const INSERT_COLUMNS = `seq, at, entity, entity_id, task_id, field, author_kind, source, actor_id, content_sha256, prev_hash, hash`;

const rowParams = (row: JournalRow) => [
	row.seq,
	row.at,
	row.entity,
	row.entity_id,
	row.task_id,
	row.field,
	row.author_kind,
	row.source,
	row.actor_id,
	row.content_sha256,
	row.prev_hash,
	row.hash,
];

const buildRows = async (
	db: LocalDb,
	author: Author,
	at: string,
	entries: JournalEntry[],
	contentShas: string[],
): Promise<JournalRow[]> => {
	const [tail] = await db.select<{ seq: number; hash: string }>(
		`SELECT seq, hash FROM edit_journal ORDER BY seq DESC LIMIT 1`,
	);
	let seq = Number(tail?.seq ?? 0);
	let prev = tail?.hash ?? GENESIS_HASH;
	const rows: JournalRow[] = [];
	for (const [index, entry] of entries.entries()) {
		seq += 1;
		const base = {
			seq,
			at,
			entity: entry.entity,
			entity_id: String(entry.entityId),
			task_id: String(entry.taskId),
			field: entry.field,
			...author,
			content_sha256: contentShas[index],
			prev_hash: prev,
		};
		const hash = await rowHash(base);
		rows.push({ ...base, hash });
		prev = hash;
	}
	return rows;
};

/**
 * Runs `statements` and appends the journal `entries` in ONE transaction (`db.batch`), so a change
 * never exists without its rows. The first journal insert only lands while the chain's tail is still
 * the one the hashes were built on; if another writer (the CLI, another window) appended first the
 * whole batch rolls back and is rebuilt on the new tail. `failedAt` is returned only for a guarded
 * statement of the caller (e.g. an UPDATE that matched nothing).
 */
export const journaledBatch = async (
	ctx: LocalContext,
	statements: BatchStatement[],
	entries: JournalEntry[],
): Promise<BatchResult> => {
	const author = authorOf(ctx);
	const at = ctx.now().toISOString();
	const shas = await Promise.all(entries.map((entry) => (entry.content === null ? '' : contentSha(entry.content))));
	for (let attempt = 0; attempt < MAX_ATTEMPTS; attempt += 1) {
		const rows = await buildRows(ctx.db, author, at, entries, shas);
		const inserts: BatchStatement[] = rows.map((row, index) =>
			index === 0
				? {
						sql: `INSERT INTO edit_journal (${INSERT_COLUMNS})
							SELECT ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?
							WHERE COALESCE((SELECT hash FROM edit_journal ORDER BY seq DESC LIMIT 1), '${GENESIS_HASH}') = ?`,
						params: [...rowParams(row), row.prev_hash],
						expectChanges: true,
				  }
				: {
						sql: `INSERT INTO edit_journal (${INSERT_COLUMNS}) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)`,
						params: rowParams(row),
				  },
		);
		const result = await ctx.db.batch([...statements, ...inserts]);
		if (result.failedAt === null) return { results: result.results.slice(0, statements.length), failedAt: null };
		if (result.failedAt < statements.length) return result;
	}
	throw new Error('The edit journal is busy; try again');
};

export const reserveId = async (db: LocalDb, table: 'tasks' | 'comments'): Promise<number> => {
	const [row] = await db.select<{ next: number }>(
		`SELECT MAX(COALESCE((SELECT seq FROM sqlite_sequence WHERE name = '${table}'), 0), COALESCE((SELECT MAX(id) FROM ${table}), 0)) + 1 AS next`,
	);
	return Number(row.next);
};

const DUPLICATE_ID = /UNIQUE constraint failed: (tasks|comments)\.id/i;

/** A create with a pre-allocated id (the id is part of the journal hash); a lost race for the id re-allocates. */
export const journaledCreate = async (
	ctx: LocalContext,
	table: 'tasks' | 'comments',
	plan: (id: number) => { statements: BatchStatement[]; entries: JournalEntry[] },
): Promise<number> => {
	for (let attempt = 0; attempt < MAX_ATTEMPTS; attempt += 1) {
		const id = await reserveId(ctx.db, table);
		const { statements, entries } = plan(id);
		try {
			await journaledBatch(ctx, statements, entries);
			return id;
		} catch (error) {
			const message = typeof error === 'string' ? error : (error as { message?: string })?.message ?? '';
			if (!DUPLICATE_ID.test(message)) throw error;
		}
	}
	throw new Error('Could not allocate an id; try again');
};

export interface ChainStatus {
	ok: boolean;
	/** The first `seq` whose row does not follow from the one before it (or from genesis). */
	brokenAt: number | null;
	rows: number;
}

const PAGE = 500;

/** Walks the whole chain in pages and reports the first row that does not verify. */
export const verifyChain = async (db: LocalDb): Promise<ChainStatus> => {
	let prevHash = GENESIS_HASH;
	let prevSeq = 0;
	let after = 0;
	let rows = 0;
	for (;;) {
		const page = await db.select<JournalRow>(
			`SELECT ${INSERT_COLUMNS} FROM edit_journal WHERE seq > ? ORDER BY seq ASC LIMIT ${PAGE}`,
			[after],
		);
		if (!page.length) return { ok: true, brokenAt: null, rows };
		for (const row of page) {
			const seq = Number(row.seq);
			if (seq !== prevSeq + 1 || row.prev_hash !== prevHash || (await rowHash({ ...row, seq })) !== row.hash) {
				return { ok: false, brokenAt: seq, rows };
			}
			prevHash = row.hash;
			prevSeq = seq;
			rows += 1;
		}
		after = prevSeq;
	}
};

export interface HumanContent {
	title_sha: string;
	description_sha: string;
	comments_sha: string;
}

export interface Provenance {
	created_by: AuthorKind | 'unrecorded';
	last_human_seq: number | null;
	last_human_content: HumanContent | null;
	modified_by_non_human_since: boolean;
	chain_ok: boolean;
	/** The task's current title, description and live comments hash to `last_human_content`. */
	content_matches: boolean;
}

/** The comments digest: SHA-256 of the JSON array `[[comment_id, content_sha256], ...]` ordered by comment id. */
export const commentsDigest = (comments: Map<number, string>): Promise<string> =>
	sha256Hex(JSON.stringify([...comments.entries()].sort((a, b) => a[0] - b[0])));

export const currentContent = async (db: LocalDb, taskId: number): Promise<HumanContent | null> => {
	const [task] = await db.select<{ title: string; description: string | null }>(
		`SELECT title, description FROM tasks WHERE id = ? AND deleted_at IS NULL`,
		[taskId],
	);
	if (!task) return null;
	const comments = await db.select<{ id: number; message: string }>(
		`SELECT id, message FROM comments WHERE task_id = ? AND deleted_at IS NULL ORDER BY id`,
		[taskId],
	);
	const shas = new Map<number, string>();
	for (const comment of comments) shas.set(Number(comment.id), await contentSha(comment.message));
	return {
		title_sha: await contentSha(task.title),
		description_sha: await contentSha(task.description),
		comments_sha: await commentsDigest(shas),
	};
};

const UNRECORDED: Provenance = {
	created_by: 'unrecorded',
	last_human_seq: null,
	last_human_content: null,
	modified_by_non_human_since: false,
	chain_ok: false,
	content_matches: false,
};

/** Who wrote a task, from the journal, checked against the content the task has right now. */
export const provenance = async (db: LocalDb, taskId: number): Promise<Provenance> => {
	const chain = await verifyChain(db);
	const rows = await db.select<JournalRow>(
		`SELECT ${INSERT_COLUMNS} FROM edit_journal WHERE task_id = ? ORDER BY seq ASC`,
		[String(taskId)],
	);
	if (!rows.length) return { ...UNRECORDED, chain_ok: chain.ok };
	const created = rows.find((row) => row.entity === 'task' && row.field === 'create');
	const humanSeqs = rows.filter((row) => row.author_kind === 'human').map((row) => Number(row.seq));
	const lastHuman = humanSeqs.length ? Math.max(...humanSeqs) : null;
	const modified = rows.some((row) => row.author_kind !== 'human' && (lastHuman === null || Number(row.seq) > lastHuman));
	let humanContent: HumanContent | null = null;
	if (lastHuman !== null) {
		let title = '';
		let description = await contentSha('');
		const comments = new Map<number, string>();
		for (const row of rows) {
			if (Number(row.seq) > lastHuman) break;
			if (row.entity === 'task' && (row.field === 'create' || row.field === 'title')) title = row.content_sha256;
			else if (row.entity === 'task' && row.field === 'description') description = row.content_sha256;
			else if (row.entity === 'comment' && row.field === 'comment') comments.set(Number(row.entity_id), row.content_sha256);
			else if (row.entity === 'comment' && row.field === 'delete') comments.delete(Number(row.entity_id));
		}
		humanContent = { title_sha: title, description_sha: description, comments_sha: await commentsDigest(comments) };
	}
	const current = await currentContent(db, taskId);
	const matches =
		!!humanContent &&
		!!current &&
		humanContent.title_sha === current.title_sha &&
		humanContent.description_sha === current.description_sha &&
		humanContent.comments_sha === current.comments_sha;
	return {
		created_by: created ? created.author_kind : 'unrecorded',
		last_human_seq: lastHuman,
		last_human_content: humanContent,
		modified_by_non_human_since: modified,
		chain_ok: chain.ok,
		content_matches: matches,
	};
};

/** Trusted as the owner's own work: created by the human, untouched by anything else since, and the chain verifies. */
export const isHumanWritten = (p: Provenance): boolean =>
	p.created_by === 'human' && !p.modified_by_non_human_since && p.chain_ok && p.content_matches;
