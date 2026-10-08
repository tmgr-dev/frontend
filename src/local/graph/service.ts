import type {
	GraphEdge,
	GraphNode,
	GraphOrphans,
	GraphPath,
	GraphRankItem,
	GraphRanking,
	GraphResult,
} from '@/types/graph';
import { LocalHttpError, type LocalContext } from '../types';
import {
	EDGE_SPEC,
	GROUP_OF,
	fetchEdges,
	kindOf,
	loadInfos,
	type Group,
	type Info,
	type RawEdge,
	type Want,
} from './data';

export const DEFAULT_LIMIT = 120;
export const MAX_LIMIT = 500;
const PER_TYPE_CAP = 24;
const HOP2_PER_PARENT_CAP = 12;
const CENTER_WEIGHT = 5;
const EXPANDING = new Set(['task', 'page']);

export interface GraphOptions {
	pagesEnabled: boolean;
}

export const ALL_GROUPS: Group[] = [
	'tasks',
	'pages',
	'people',
	'personas',
	'agent_runs',
	'comments',
];
export const DEFAULT_GROUPS: Group[] = ALL_GROUPS.filter(
	(g) => g !== 'comments',
);

const notFound = () => new LocalHttpError(404, 'Entity not found');

const refOf = (info: Info) => info.node.key ?? info.node.title;

const whyOf = (type: string, a: Info, b: Info): string => {
	const x = refOf(a);
	const y = refOf(b);
	switch (type) {
		case 'blocks':
		case 'depends_on':
		case 'duplicates':
		case 'relates_to':
			return `${x} ${EDGE_SPEC[type].label} ${y} (task relation)`;
		case 'links_to':
			return `${x} links to ${y}`;
		case 'linked_page':
			return `${x} links ${y}`;
		case 'mentioned_in':
			return `${x} is mentioned in ${y}`;
		case 'mentions':
			return `${x} mentions ${y}`;
		case 'persona_assignee':
			return `${x} is assigned to ${y}`;
		case 'commented':
			return `${x} commented on ${y}`;
		default:
			return `${x} is on ${y}`;
	}
};

const edgeId = (e: RawEdge) => `${e.type}:${e.from}:${e.to}`;

const rank = (
	a: { weight: number; updated: string; id: string },
	b: { weight: number; updated: string; id: string },
) =>
	b.weight - a.weight ||
	(a.updated < b.updated ? 1 : a.updated > b.updated ? -1 : 0) ||
	(a.id < b.id ? -1 : a.id > b.id ? 1 : 0);

const wantOf = (groups: Set<Group>, pagesEnabled: boolean): Want => ({
	pages: pagesEnabled && groups.has('pages'),
	people: groups.has('people'),
	personas: groups.has('personas'),
	comments: groups.has('comments'),
});

class Resolver {
	infos = new Map<string, Info | null>();

	constructor(private ctx: LocalContext, private pagesEnabled: boolean) {}

	async ensure(ids: Iterable<string>) {
		const missing = [...new Set(ids)].filter((id) => !this.infos.has(id));
		if (!missing.length) return;
		const loaded = await loadInfos(this.ctx, missing, this.pagesEnabled);
		for (const id of missing) this.infos.set(id, loaded.get(id) ?? null);
	}

	get(id: string): Info | null {
		return this.infos.get(id) ?? null;
	}

	async buildEdges(
		raw: RawEdge[],
		allowed: (id: string) => boolean,
	): Promise<Map<string, GraphEdge>> {
		await this.ensure(raw.flatMap((e) => [e.from, e.to]));
		const out = new Map<string, GraphEdge>();
		for (const e of raw) {
			const a = this.get(e.from);
			const b = this.get(e.to);
			if (!a || !b || !allowed(e.from) || !allowed(e.to)) continue;
			const id = edgeId(e);
			if (out.has(id)) continue;
			const spec = EDGE_SPEC[e.type];
			out.set(id, {
				id,
				from: e.from,
				to: e.to,
				type: e.type,
				label: spec.label,
				weight: spec.weight,
				why: whyOf(e.type, a, b),
			});
		}
		return out;
	}
}

export const resolveEntity = async (
	ctx: LocalContext,
	entity: string,
	options: GraphOptions,
): Promise<string> => {
	const value = (entity ?? '').trim();
	let id: string | null = null;
	let m: RegExpExecArray | null;
	if ((m = /^task:(\d+)$/.exec(value))) id = `task:${Number(m[1])}`;
	else if ((m = /^page:(\d+)$/.exec(value))) id = `page:${Number(m[1])}`;
	else if ((m = /^page:(.+)$/.exec(value))) {
		if (!options.pagesEnabled) throw notFound();
		const [row] = await ctx.db.select<{ id: number }>(
			`SELECT id FROM pages WHERE slug = ? AND deleted_at IS NULL`,
			[m[1]],
		);
		if (!row) throw notFound();
		id = `page:${row.id}`;
	} else if ((m = /^([A-Za-z][A-Za-z0-9_]*)-(\d+)$/.exec(value))) {
		const [row] = await ctx.db.select<{ id: number }>(
			`SELECT t.id FROM tasks t JOIN categories c ON c.id = t.project_category_id
			 WHERE UPPER(c.code) = UPPER(?) AND t.category_tasks_sequence_id = ? AND t.deleted_at IS NULL`,
			[m[1], Number(m[2])],
		);
		if (!row) throw notFound();
		id = `task:${row.id}`;
	}
	if (!id)
		throw new LocalHttpError(
			422,
			'entity must be task:<id>, page:<id>, page:<slug> or a task key',
		);
	const loaded = await loadInfos(ctx, [id], options.pagesEnabled);
	if (!loaded.has(id)) throw notFound();
	return id;
};

export interface RelatedInput {
	entity: string;
	depth?: number;
	include?: Group[];
	limit?: number;
}

export const related = async (
	ctx: LocalContext,
	input: RelatedInput,
	options: GraphOptions,
): Promise<GraphResult> => {
	const depth = input.depth === 1 ? 1 : 2;
	const limit = Math.min(
		MAX_LIMIT,
		Math.max(1, Math.floor(input.limit ?? DEFAULT_LIMIT)),
	);
	const groups = new Set<Group>(
		input.include?.length ? input.include : DEFAULT_GROUPS,
	);
	const center = await resolveEntity(ctx, input.entity, options);
	const resolver = new Resolver(ctx, options.pagesEnabled);
	await resolver.ensure([center]);
	const allowed = (id: string) =>
		id === center ||
		((kindOf(id) !== 'page' || options.pagesEnabled) &&
			groups.has(GROUP_OF[kindOf(id)]));
	const want = wantOf(groups, options.pagesEnabled);

	const nodes = new Map<string, GraphNode>();
	const centerNode = { ...resolver.get(center)!.node, weight: CENTER_WEIGHT };
	nodes.set(center, centerNode);
	const pool = new Map<string, GraphEdge>();
	const dropped = new Map<string, GraphNode['type']>();
	let frontier = [center];

	for (let hop = 1; hop <= depth && frontier.length; hop++) {
		const scope = {
			tasks: frontier
				.filter((id) => kindOf(id) === 'task')
				.map((id) => Number(id.slice(5))),
			pages: frontier
				.filter((id) => kindOf(id) === 'page')
				.map((id) => Number(id.slice(5))),
		};
		const built = await resolver.buildEdges(
			await fetchEdges(ctx, scope, want),
			allowed,
		);
		built.forEach((e, id) => pool.set(id, e));

		const inFrontier = new Set(frontier);
		interface Cand {
			parent: string;
			neighbor: string;
			edge: GraphEdge;
			updated: string;
		}
		const cands: Cand[] = [];
		for (const e of built.values())
			for (const [parent, neighbor] of [
				[e.from, e.to],
				[e.to, e.from],
			])
				if (inFrontier.has(parent))
					cands.push({
						parent,
						neighbor,
						edge: e,
						updated: resolver.get(neighbor)!.updated,
					});
		const order = (a: Cand, b: Cand) =>
			rank(
				{ weight: a.edge.weight, updated: a.updated, id: a.neighbor },
				{ weight: b.edge.weight, updated: b.updated, id: b.neighbor },
			);
		cands.sort(order);

		const takeNew = (list: Cand[], cap: number): Cand[] => {
			const seen = new Set<string>();
			const kept: Cand[] = [];
			for (const c of list) {
				if (nodes.has(c.neighbor)) {
					kept.push(c);
					continue;
				}
				if (seen.has(c.neighbor)) {
					kept.push(c);
					continue;
				}
				if (seen.size < cap) {
					seen.add(c.neighbor);
					kept.push(c);
				} else dropped.set(c.neighbor, kindOf(c.neighbor));
			}
			return kept;
		};
		const groupBy = (list: Cand[], key: (c: Cand) => string) => {
			const map = new Map<string, Cand[]>();
			for (const c of list) {
				const k = key(c);
				map.set(k, [...(map.get(k) ?? []), c]);
			}
			return [...map.values()];
		};

		let kept = groupBy(cands, (c) => `${c.parent}|${c.edge.type}`).flatMap(
			(list) => takeNew(list, PER_TYPE_CAP),
		);
		if (hop >= 2)
			kept = groupBy(kept, (c) => c.parent).flatMap((list) =>
				takeNew(list, HOP2_PER_PARENT_CAP),
			);
		kept.sort(order);

		const fresh = new Map<string, Cand>();
		for (const c of kept)
			if (!nodes.has(c.neighbor) && !fresh.has(c.neighbor))
				fresh.set(c.neighbor, c);
		const next: string[] = [];
		for (const [id, c] of fresh) {
			if (nodes.size >= limit) {
				dropped.set(id, kindOf(id));
				continue;
			}
			const node: GraphNode = {
				...resolver.get(id)!.node,
				hop,
				weight: c.edge.weight,
				rel: c.edge.label,
			};
			nodes.set(id, node);
			if (EXPANDING.has(node.type)) next.push(id);
		}
		frontier = next;
	}

	const capsHit: Record<string, number> = {};
	for (const [id, type] of dropped)
		if (!nodes.has(id)) capsHit[type] = (capsHit[type] ?? 0) + 1;

	const updatedOf = (id: string) => resolver.get(id)?.updated ?? '';
	const ordered = [...nodes.values()].sort((a, b) =>
		a.id === center
			? -1
			: b.id === center
			? 1
			: a.hop - b.hop ||
			  rank(
					{ weight: a.weight, updated: updatedOf(a.id), id: a.id },
					{ weight: b.weight, updated: updatedOf(b.id), id: b.id },
			  ),
	);
	const edges = [...pool.values()]
		.filter((e) => nodes.has(e.from) && nodes.has(e.to))
		.sort((a, b) => b.weight - a.weight || (a.id < b.id ? -1 : 1));
	return {
		center,
		nodes: ordered,
		edges,
		truncated: Object.keys(capsHit).length > 0,
		caps_hit: capsHit,
	};
};

const TASK_PAGE: Group[] = ['tasks', 'pages'];

const taskPageAllowed = (id: string) => EXPANDING.has(kindOf(id));

export const path = async (
	ctx: LocalContext,
	input: { from: string; to: string; max_depth?: number },
	options: GraphOptions,
): Promise<GraphPath> => {
	const maxDepth = Math.min(6, Math.max(1, Math.floor(input.max_depth ?? 4)));
	const from = await resolveEntity(ctx, input.from, options);
	const to = await resolveEntity(ctx, input.to, options);
	const resolver = new Resolver(ctx, options.pagesEnabled);
	await resolver.ensure([from, to]);
	const want = wantOf(new Set(TASK_PAGE), options.pagesEnabled);

	const parent = new Map<string, { prev: string; edge: GraphEdge } | null>();
	parent.set(from, null);
	let frontier = [from];
	for (
		let hop = 1;
		hop <= maxDepth && frontier.length && !parent.has(to);
		hop++
	) {
		const built = await resolver.buildEdges(
			await fetchEdges(
				ctx,
				{
					tasks: frontier
						.filter((id) => kindOf(id) === 'task')
						.map((id) => Number(id.slice(5))),
					pages: frontier
						.filter((id) => kindOf(id) === 'page')
						.map((id) => Number(id.slice(5))),
				},
				want,
			),
			taskPageAllowed,
		);
		const inFrontier = new Set(frontier);
		const found = new Map<string, { prev: string; edge: GraphEdge }>();
		const sorted = [...built.values()].sort(
			(a, b) => b.weight - a.weight || (a.id < b.id ? -1 : 1),
		);
		for (const e of sorted)
			for (const [prev, next] of [
				[e.from, e.to],
				[e.to, e.from],
			])
				if (inFrontier.has(prev) && !parent.has(next) && !found.has(next))
					found.set(next, { prev, edge: e });
		const nextFrontier = [...found.keys()].sort();
		nextFrontier.forEach((id) => parent.set(id, found.get(id)!));
		frontier = nextFrontier;
	}
	if (!parent.has(to)) return { from, to, nodes: [], edges: [] };

	const chain: string[] = [];
	const edges: GraphEdge[] = [];
	for (let cur: string | null = to; cur; ) {
		chain.unshift(cur);
		const step: { prev: string; edge: GraphEdge } | null =
			parent.get(cur) ?? null;
		if (step) edges.unshift(step.edge);
		cur = step ? step.prev : null;
	}
	const nodes = chain.map(
		(id, hop): GraphNode => ({
			...resolver.get(id)!.node,
			hop,
			weight: hop === 0 ? CENTER_WEIGHT : edges[hop - 1].weight,
			rel: hop === 0 ? null : edges[hop - 1].label,
		}),
	);
	return { from, to, nodes, edges };
};

interface Graphed {
	resolver: Resolver;
	edges: GraphEdge[];
	degree: Map<string, Set<string>>;
}

const workspaceGraph = async (
	ctx: LocalContext,
	options: GraphOptions,
): Promise<Graphed> => {
	const resolver = new Resolver(ctx, options.pagesEnabled);
	const want = wantOf(new Set(TASK_PAGE), options.pagesEnabled);
	const built = await resolver.buildEdges(
		await fetchEdges(ctx, 'all', want),
		taskPageAllowed,
	);
	const edges = [...built.values()];
	const degree = new Map<string, Set<string>>();
	for (const e of edges)
		for (const id of [e.from, e.to])
			degree.set(id, (degree.get(id) ?? new Set()).add(e.id));
	return { resolver, edges, degree };
};

const clampLimit = (value: number | undefined, fallback: number, max: number) =>
	Math.min(max, Math.max(1, Math.floor(value ?? fallback)));

export const hubs = async (
	ctx: LocalContext,
	input: { limit?: number },
	options: GraphOptions,
): Promise<GraphRanking> => {
	const limit = clampLimit(input.limit, 10, 100);
	const { resolver, edges, degree } = await workspaceGraph(ctx, options);
	const dependants = new Map<string, Set<string>>();
	const add = (blocker: string, blocked: string) =>
		dependants.set(
			blocker,
			(dependants.get(blocker) ?? new Set()).add(blocked),
		);
	for (const e of edges) {
		if (e.type === 'blocks') add(e.from, e.to);
		else if (e.type === 'depends_on') add(e.to, e.from);
	}
	const item = (id: string): GraphRankItem => {
		const info = resolver.get(id)!;
		const d = degree.get(id)?.size ?? 0;
		return {
			node: { ...info.node, weight: d },
			degree: d,
			blocks: dependants.get(id)?.size ?? 0,
		};
	};
	const rankBy = (ids: string[], key: (i: GraphRankItem) => number[]) =>
		ids
			.map(item)
			.sort((a, b) => {
				const ka = key(a);
				const kb = key(b);
				for (let i = 0; i < ka.length; i++)
					if (ka[i] !== kb[i]) return kb[i] - ka[i];
				const ua = resolver.get(a.node.id)!.updated;
				const ub = resolver.get(b.node.id)!.updated;
				return (
					(ua < ub ? 1 : ua > ub ? -1 : 0) || (a.node.id < b.node.id ? -1 : 1)
				);
			})
			.slice(0, limit);
	return {
		hubs: rankBy([...degree.keys()], (i) => [i.degree]),
		bottlenecks: rankBy([...dependants.keys()], (i) => [i.blocks, i.degree]),
	};
};

export const orphans = async (
	ctx: LocalContext,
	input: { limit?: number },
	options: GraphOptions,
): Promise<GraphOrphans> => {
	const limit = clampLimit(input.limit, 50, MAX_LIMIT);
	const { degree } = await workspaceGraph(ctx, options);
	const rows = await ctx.db.select<{ id: string; updated_at: string }>(
		`SELECT 'task:' || t.id AS id, t.updated_at
		 FROM tasks t LEFT JOIN statuses s ON s.id = t.status_id
		 WHERE t.deleted_at IS NULL AND (s.type IS NULL OR s.type NOT IN ('completed', 'archived'))
		 ${
				options.pagesEnabled
					? `UNION ALL SELECT 'page:' || id, updated_at FROM pages WHERE deleted_at IS NULL`
					: ''
			}`,
	);
	const loose = rows
		.filter((r) => !degree.has(r.id))
		.sort((a, b) =>
			rank(
				{ weight: 0, updated: a.updated_at, id: a.id },
				{ weight: 0, updated: b.updated_at, id: b.id },
			),
		);
	const slice = loose.slice(0, limit).map((r) => r.id);
	const infos = await loadInfos(ctx, slice, options.pagesEnabled);
	return {
		nodes: slice.filter((id) => infos.has(id)).map((id) => infos.get(id)!.node),
		total: loose.length,
	};
};
