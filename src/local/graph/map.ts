import type {
	GraphBridge,
	GraphEdge,
	GraphMap,
	GraphMapCategory,
	GraphNode,
	GraphRankItem,
} from '@/types/graph';
import { LocalHttpError, type LocalContext } from '../types';
import { EDGE_SPEC, fetchEdges, kindOf, loadInfos } from './data';
import type { GraphOptions } from './service';

const DEFAULT_LIMIT = 3000;
const MAX_LIMIT = 5000;
const ORPHAN_IDS = 200;
const CLOSED = new Set(['completed', 'archived']);
const DAY = /^\d{4}-\d{2}-\d{2}$/;

const validDay = (value: string | undefined, name: string) => {
	if (value === undefined || value === '') return undefined;
	if (!DAY.test(value))
		throw new LocalHttpError(422, `${name} must be YYYY-MM-DD`);
	return value;
};

const desc = (a: string, b: string) => (a < b ? 1 : a > b ? -1 : 0);

export const map = async (
	ctx: LocalContext,
	input: { from?: string; to?: string; limit?: number },
	options: GraphOptions,
): Promise<GraphMap> => {
	const from = validDay(input.from, 'from');
	const to = validDay(input.to, 'to');
	const limit = Math.min(
		MAX_LIMIT,
		Math.max(1, Math.floor(input.limit ?? DEFAULT_LIMIT)),
	);
	const params: string[] = [];
	let window = '';
	if (to) {
		window += ' AND created_at <= ?';
		params.push(`${to}T23:59:59.999Z`);
	}
	if (from) {
		window += ' AND updated_at >= ?';
		params.push(from);
	}
	const rows = await ctx.db.select<{
		id: string;
		created_at: string;
		updated_at: string;
	}>(
		`SELECT 'task:' || id AS id, created_at, updated_at FROM tasks WHERE deleted_at IS NULL${window}
		 ${
				options.pagesEnabled
					? `UNION ALL SELECT 'page:' || id, created_at, updated_at FROM pages WHERE deleted_at IS NULL${window}`
					: ''
			}`,
		options.pagesEnabled ? [...params, ...params] : params,
	);
	const matching = new Map(rows.map((r) => [r.id, r]));

	const raw = await fetchEdges(ctx, 'all', {
		pages: options.pagesEnabled,
		people: false,
		personas: false,
		comments: false,
	});
	const specs = new Map<string, GraphEdge>();
	const degreeAll = new Map<string, number>();
	for (const e of raw) {
		if (!matching.has(e.from) || !matching.has(e.to)) continue;
		const id = `${e.type}:${e.from}:${e.to}`;
		if (specs.has(id)) continue;
		const spec = EDGE_SPEC[e.type];
		specs.set(id, {
			id,
			from: e.from,
			to: e.to,
			type: e.type,
			label: spec.label,
			weight: spec.weight,
			why: '',
		});
		degreeAll.set(e.from, (degreeAll.get(e.from) ?? 0) + 1);
		degreeAll.set(e.to, (degreeAll.get(e.to) ?? 0) + 1);
	}

	const chosenIds = [...matching.values()]
		.sort(
			(a, b) =>
				(degreeAll.get(b.id) ?? 0) - (degreeAll.get(a.id) ?? 0) ||
				desc(a.updated_at, b.updated_at) ||
				(a.id < b.id ? -1 : 1),
		)
		.slice(0, limit)
		.map((r) => r.id);
	const chosen = new Set(chosenIds);
	const edges = [...specs.values()]
		.filter((e) => chosen.has(e.from) && chosen.has(e.to))
		.sort((a, b) => b.weight - a.weight || (a.id < b.id ? -1 : 1));

	const degree = new Map<string, number>();
	const dependants = new Map<string, Set<string>>();
	const infos = await loadInfos(ctx, chosenIds, options.pagesEnabled);
	const isOpen = (id: string) => {
		const type = infos.get(id)?.node.status?.type;
		return !type || !CLOSED.has(type);
	};
	for (const e of edges) {
		degree.set(e.from, (degree.get(e.from) ?? 0) + 1);
		degree.set(e.to, (degree.get(e.to) ?? 0) + 1);
		if (e.type !== 'blocks' && e.type !== 'depends_on') continue;
		const [blocker, blocked] =
			e.type === 'blocks' ? [e.from, e.to] : [e.to, e.from];
		if (isOpen(blocker) && isOpen(blocked))
			dependants.set(
				blocker,
				(dependants.get(blocker) ?? new Set()).add(blocked),
			);
	}

	const nodes: GraphNode[] = [];
	for (const id of chosenIds) {
		const info = infos.get(id);
		if (!info) continue;
		const row = matching.get(id)!;
		const d = degree.get(id) ?? 0;
		nodes.push({
			...info.node,
			weight: d,
			meta: {
				...info.node.meta,
				created_at: row.created_at,
				updated_at: row.updated_at,
				degree: d,
				blocks: dependants.get(id)?.size ?? 0,
			},
		});
	}
	const byId = new Map(nodes.map((n) => [n.id, n]));
	const updatedOf = (n: GraphNode) => String(n.meta.updated_at);
	const item = (n: GraphNode): GraphRankItem => ({
		node: n,
		degree: Number(n.meta.degree),
		blocks: Number(n.meta.blocks),
	});
	const tie = (a: GraphNode, b: GraphNode) =>
		desc(updatedOf(a), updatedOf(b)) || (a.id < b.id ? -1 : 1);

	const hubs = nodes
		.filter((n) => n.weight > 0)
		.sort((a, b) => b.weight - a.weight || tie(a, b))
		.slice(0, 5)
		.map(item);
	const bottlenecks = nodes
		.filter((n) => Number(n.meta.blocks) > 0)
		.sort(
			(a, b) =>
				Number(b.meta.blocks) - Number(a.meta.blocks) ||
				b.weight - a.weight ||
				tie(a, b),
		)
		.slice(0, 5)
		.map(item);

	const orphanNodes = nodes
		.filter(
			(n) => n.weight === 0 && !(n.status?.type && CLOSED.has(n.status.type)),
		)
		.sort(tie);

	const categoryOf = (id: string) => byId.get(id)?.category_id ?? null;
	const pairs = new Map<string, GraphBridge>();
	for (const e of edges) {
		if (kindOf(e.from) !== 'task' || kindOf(e.to) !== 'task') continue;
		const a = categoryOf(e.from);
		const b = categoryOf(e.to);
		if (a === b) continue;
		const [lo, hi] = a === null || (b !== null && a < b) ? [a, b] : [b, a];
		const key = `${lo}|${hi}`;
		const entry = pairs.get(key) ?? {
			from_category_id: lo,
			to_category_id: hi,
			edges: 0,
		};
		entry.edges += 1;
		pairs.set(key, entry);
	}
	const bridges = [...pairs.values()]
		.sort(
			(a, b) =>
				b.edges - a.edges ||
				(a.from_category_id ?? -1) - (b.from_category_id ?? -1) ||
				(a.to_category_id ?? -1) - (b.to_category_id ?? -1),
		)
		.slice(0, 6);

	const counts = new Map<number | null, number>();
	for (const n of nodes)
		if (n.type === 'task')
			counts.set(n.category_id, (counts.get(n.category_id) ?? 0) + 1);
	const meta = await ctx.db.select<{
		id: number;
		title: string;
		code: string | null;
	}>(`SELECT id, title, code FROM categories`);
	const categories: GraphMapCategory[] = [...counts.entries()]
		.map(([id, count]) => {
			const c = meta.find((m) => m.id === id);
			return {
				id,
				title: id === null ? 'Uncategorized' : c?.title ?? String(id),
				code: c?.code ?? null,
				count,
			};
		})
		.sort((a, b) => b.count - a.count || (a.id ?? -1) - (b.id ?? -1));

	return {
		nodes,
		edges,
		categories,
		insights: {
			hubs,
			bottlenecks,
			bridges,
			orphans: {
				total: orphanNodes.length,
				ids: orphanNodes.slice(0, ORPHAN_IDS).map((n) => n.id),
			},
		},
		truncated: matching.size > limit,
		total: matching.size,
	};
};
