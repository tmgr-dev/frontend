import type { GraphNode, GraphNodeType } from '@/types/graph';
import type { LocalContext } from '../types';

export type Group =
	| 'tasks'
	| 'pages'
	| 'people'
	| 'personas'
	| 'agent_runs'
	| 'comments';

export const GROUP_OF: Record<GraphNodeType, Group> = {
	task: 'tasks',
	page: 'pages',
	user: 'people',
	persona: 'personas',
	agent_run: 'agent_runs',
	comment: 'comments',
};

export const kindOf = (id: string): GraphNodeType =>
	id.slice(0, id.indexOf(':')) as GraphNodeType;

export const EDGE_SPEC: Record<string, { label: string; weight: number }> = {
	blocks: { label: 'blocks', weight: 3 },
	depends_on: { label: 'depends on', weight: 3 },
	duplicates: { label: 'duplicates', weight: 2 },
	relates_to: { label: 'relates to', weight: 2 },
	links_to: { label: 'links to', weight: 2 },
	linked_page: { label: 'linked page', weight: 2 },
	mentioned_in: { label: 'mentioned in', weight: 2 },
	mentions: { label: 'mentions', weight: 1 },
	persona_assignee: { label: 'assignee', weight: 1 },
	commented: { label: 'commented', weight: 1 },
	on_task: { label: 'on task', weight: 1 },
};

export interface RawEdge {
	type: string;
	from: string;
	to: string;
}

export interface Info {
	node: GraphNode;
	updated: string;
}

export interface Scope {
	tasks: number[];
	pages: number[];
}

export interface Want {
	pages: boolean;
	people: boolean;
	personas: boolean;
	comments: boolean;
}

const CHUNK = 400;

const chunks = <T>(items: T[]): T[][] => {
	const out: T[][] = [];
	for (let i = 0; i < items.length; i += CHUNK)
		out.push(items.slice(i, i + CHUNK));
	return out;
};

const byIds = async <R>(
	ctx: LocalContext,
	sql: string,
	ids: (string | number)[],
): Promise<R[]> => {
	const rows: R[] = [];
	for (const part of chunks(ids)) {
		const marks = `(${part.map(() => '?').join(',')})`;
		rows.push(...(await ctx.db.select<R>(sql.replace('{IN}', marks), part)));
	}
	return rows;
};

const RELATION_KINDS: Record<string, { type: string; reverse: boolean }> = {
	blocks: { type: 'blocks', reverse: false },
	blocking: { type: 'blocks', reverse: false },
	'is blocked by': { type: 'blocks', reverse: true },
	'blocked by': { type: 'blocks', reverse: true },
	'depends on': { type: 'depends_on', reverse: false },
	'is dependency of': { type: 'depends_on', reverse: true },
	duplicates: { type: 'duplicates', reverse: false },
	'is duplicated by': { type: 'duplicates', reverse: true },
	'relates to': { type: 'relates_to', reverse: false },
	related: { type: 'relates_to', reverse: false },
};

const relationEdge = (a: number, b: number, name: string): RawEdge | null => {
	const kind = RELATION_KINDS[name.trim().toLowerCase()];
	if (!kind) return null;
	let [from, to] = kind.reverse ? [b, a] : [a, b];
	if (kind.type === 'relates_to' && from > to) [from, to] = [to, from];
	return { type: kind.type, from: `task:${from}`, to: `task:${to}` };
};

export const fetchEdges = async (
	ctx: LocalContext,
	scope: Scope | 'all',
	want: Want,
): Promise<RawEdge[]> => {
	const out: RawEdge[] = [];
	const all = scope === 'all';
	const tasks = all ? [] : scope.tasks;
	const pages = all ? [] : scope.pages;
	const pull = async <R>(
		sql: string,
		wheres: { where: string; ids: number[] }[],
	): Promise<R[]> => {
		if (all) return ctx.db.select<R>(sql);
		const rows: R[] = [];
		for (const w of wheres)
			rows.push(...(await byIds<R>(ctx, `${sql} WHERE ${w.where}`, w.ids)));
		return rows;
	};

	const relations = await pull<any>(
		`SELECT r.task_id AS a, r.related_task_id AS b, rt.name AS name
		 FROM task_relations r JOIN task_relation_types rt ON rt.id = r.relation_type_id`,
		[
			{ where: 'r.task_id IN {IN}', ids: tasks },
			{ where: 'r.related_task_id IN {IN}', ids: tasks },
		],
	);
	for (const r of relations) {
		const edge = relationEdge(r.a, r.b, r.name);
		if (edge) out.push(edge);
	}

	if (want.pages) {
		const links = await pull<any>(
			`SELECT page_id, target_kind, target_id FROM page_links`,
			[
				{ where: 'page_id IN {IN}', ids: pages },
				{ where: `target_kind = 'page' AND target_id IN {IN}`, ids: pages },
				{ where: `target_kind = 'task' AND target_id IN {IN}`, ids: tasks },
			],
		);
		for (const l of links) {
			const from = `page:${l.page_id}`;
			if (l.target_kind === 'page')
				out.push({ type: 'links_to', from, to: `page:${l.target_id}` });
			else if (l.target_kind === 'task')
				out.push({ type: 'linked_page', from, to: `task:${l.target_id}` });
			else if (l.target_kind === 'user' && want.people)
				out.push({ type: 'mentions', from, to: `user:${l.target_id}` });
		}
		const mentions = await pull<any>(
			`SELECT task_id, page_id FROM task_page_mentions`,
			[
				{ where: 'task_id IN {IN}', ids: tasks },
				{ where: 'page_id IN {IN}', ids: pages },
			],
		);
		for (const m of mentions)
			out.push({
				type: 'mentioned_in',
				from: `task:${m.task_id}`,
				to: `page:${m.page_id}`,
			});
	}

	if (!all && want.personas) {
		const rowsP = await byIds<any>(
			ctx,
			`SELECT task_id, persona_uuid FROM task_persona_assignees WHERE task_id IN {IN}`,
			tasks,
		);
		for (const r of rowsP)
			out.push({
				type: 'persona_assignee',
				from: `task:${r.task_id}`,
				to: `persona:${r.persona_uuid}`,
			});
	}

	if (!all && want.comments) {
		const rowsC = await byIds<any>(
			ctx,
			`SELECT id, task_id, author_kind FROM comments WHERE deleted_at IS NULL AND task_id IN {IN}`,
			tasks,
		);
		for (const c of rowsC) {
			out.push({
				type: 'on_task',
				from: `comment:${c.id}`,
				to: `task:${c.task_id}`,
			});
			if (want.people && (c.author_kind ?? 'user') === 'user')
				out.push({
					type: 'commented',
					from: `user:${ctx.user.id}`,
					to: `task:${c.task_id}`,
				});
		}
	}
	return out;
};

const baseNode = (
	id: string,
	type: GraphNodeType,
	refId: number | string,
	title: string,
): GraphNode => ({
	id,
	type,
	ref_id: refId,
	key: null,
	title,
	status: null,
	category_id: null,
	category: null,
	hop: 0,
	weight: 0,
	rel: null,
	meta: {},
});

const checkpointMeta = (raw: string | null) => {
	if (!raw) return {};
	try {
		const list = JSON.parse(raw);
		if (!Array.isArray(list) || !list.length) return {};
		const done = list.filter((c) => c?.checked).length;
		return { checkpoints: { done, open: list.length - done } };
	} catch {
		return {};
	}
};

const SNIPPET = 80;

export const loadInfos = async (
	ctx: LocalContext,
	ids: string[],
	pagesEnabled: boolean,
): Promise<Map<string, Info>> => {
	const out = new Map<string, Info>();
	const pick = (kind: GraphNodeType) =>
		ids
			.filter((id) => kindOf(id) === kind)
			.map((id) => id.slice(kind.length + 1));

	const taskIds = pick('task').filter((s) => /^\d+$/.test(s));
	for (const r of await byIds<any>(
		ctx,
		`SELECT t.id, t.title, t.updated_at, t.checkpoints, t.category_tasks_sequence_id AS seq,
			t.project_category_id AS cid, c.title AS ctitle, c.code AS ccode,
			s.name AS sname, s.type AS stype
		 FROM tasks t
		 LEFT JOIN statuses s ON s.id = t.status_id
		 LEFT JOIN categories c ON c.id = t.project_category_id
		 WHERE t.deleted_at IS NULL AND t.id IN {IN}`,
		taskIds.map(Number),
	)) {
		const node = baseNode(`task:${r.id}`, 'task', r.id, r.title);
		node.key = r.ccode && r.seq ? `${r.ccode}-${r.seq}` : null;
		node.status = r.sname ? { name: r.sname, type: r.stype } : null;
		node.category_id = r.cid ?? null;
		node.category = r.ctitle ?? null;
		node.meta = checkpointMeta(r.checkpoints);
		out.set(node.id, { node, updated: r.updated_at ?? '' });
	}

	if (pagesEnabled) {
		const pageIds = pick('page').filter((s) => /^\d+$/.test(s));
		for (const r of await byIds<any>(
			ctx,
			`SELECT id, title, slug, type, updated_at FROM pages WHERE deleted_at IS NULL AND id IN {IN}`,
			pageIds.map(Number),
		)) {
			const node = baseNode(`page:${r.id}`, 'page', r.id, r.title);
			node.meta = { slug: r.slug, page_type: r.type };
			out.set(node.id, { node, updated: r.updated_at ?? '' });
		}
	}

	for (const r of await byIds<any>(
		ctx,
		`SELECT p.uuid, p.name, p.synced_at
		 FROM personas p JOIN workspace_personas wp ON wp.persona_uuid = p.uuid
		 WHERE wp.disabled_at IS NULL AND p.archived_at IS NULL AND p.uuid IN {IN}`,
		pick('persona'),
	)) {
		const node = baseNode(
			`persona:${r.uuid}`,
			'persona',
			r.uuid,
			r.name || String(r.uuid).slice(0, 8),
		);
		out.set(node.id, { node, updated: r.synced_at ?? '' });
	}

	for (const r of await byIds<any>(
		ctx,
		`SELECT id, message, updated_at FROM comments WHERE deleted_at IS NULL AND id IN {IN}`,
		pick('comment')
			.filter((s) => /^\d+$/.test(s))
			.map(Number),
	)) {
		const text = String(r.message ?? '')
			.replace(/\s+/g, ' ')
			.trim();
		const node = baseNode(
			`comment:${r.id}`,
			'comment',
			r.id,
			text.length > SNIPPET ? `${text.slice(0, SNIPPET)}…` : text,
		);
		out.set(node.id, { node, updated: r.updated_at ?? '' });
	}

	if (pick('user').includes(String(ctx.user.id))) {
		const node = baseNode(
			`user:${ctx.user.id}`,
			'user',
			ctx.user.id,
			ctx.user.name,
		);
		out.set(node.id, { node, updated: '' });
	}
	return out;
};
