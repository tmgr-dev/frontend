import type { GraphEdge, GraphMap, GraphNode } from '@/types/graph';
import { hashString, truncateLabel } from '../graphLogic';
import type { LayoutInit } from './mapLayoutCore';

export const PAGES_CLUSTER = 'pages';
export const NONE_CLUSTER = 'none';
export const PAGES_TITLE = 'Pages & docs';
export const NONE_TITLE = 'No category';
export const BOTTLENECK_COLOR = '#ff8a4c';
export const DAY_MS = 86400000;
export const MAP_LEGEND =
	'Clusters = categories · Bright curves = links between areas · Dashed rings = items with no links · Orange = bottleneck';

export type RangeKey = '7' | '30' | '90' | 'all';

export const RANGES: { key: RangeKey; label: string; days: number | null }[] = [
	{ key: '7', label: 'Last 7 days', days: 7 },
	{ key: '30', label: 'Last 30 days', days: 30 },
	{ key: '90', label: 'Last 90 days', days: 90 },
	{ key: 'all', label: 'All time', days: null },
];

const pad = (n: number) => String(n).padStart(2, '0');
export const ymd = (d: Date): string =>
	`${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}`;

export const rangeBounds = (
	key: RangeKey,
	now: Date,
): { from: string | null; to: string } => {
	const days = RANGES.find((r) => r.key === key)?.days ?? null;
	const to = ymd(now);
	if (days === null) return { from: null, to };
	const start = new Date(now.getFullYear(), now.getMonth(), now.getDate());
	start.setDate(start.getDate() - days);
	return { from: ymd(start), to };
};

const parseMs = (value: unknown): number => {
	const ms = typeof value === 'string' ? Date.parse(value) : NaN;
	return Number.isFinite(ms) ? ms : NaN;
};

export const inRange = (
	createdMs: number,
	updatedMs: number,
	fromMs: number | null,
	toMs: number,
): boolean =>
	(Number.isNaN(createdMs) || createdMs <= toMs) &&
	(fromMs === null || Number.isNaN(updatedMs) || updatedMs >= fromMs);

export const endOfDayMs = (date: string): number =>
	new Date(`${date}T23:59:59.999`).getTime();
export const startOfDayMs = (date: string): number =>
	new Date(`${date}T00:00:00`).getTime();

export const visibleAt = (createdMs: number, cursorMs: number): boolean =>
	Number.isNaN(createdMs) || createdMs <= cursorMs;

export const timelineSpan = (
	createdMs: ArrayLike<number>,
	fromMs: number | null,
	toMs: number,
): { start: number; end: number } => {
	let start = fromMs;
	if (start === null) {
		let min = Infinity;
		for (let i = 0; i < createdMs.length; i++) {
			if (createdMs[i] < min) min = createdMs[i];
		}
		start = Number.isFinite(min) ? min : toMs - 7 * DAY_MS;
	}
	if (start >= toMs) start = toMs - 7 * DAY_MS;
	return { start, end: toMs };
};

export const timelineSteps = (start: number, end: number): number[] => {
	const step = end - start <= 14 * DAY_MS ? DAY_MS : 7 * DAY_MS;
	const out: number[] = [];
	for (let t = start; t < end; t += step) out.push(t);
	out.push(end);
	return out;
};

export const PAGES_COLOR = '#f0a646';
const PALETTE = [
	'#5b8cff',
	'#b493ff',
	'#f472b6',
	'#3ccfb6',
	'#7aa2ff',
	'#8bd450',
	'#56b6c2',
	'#c678dd',
	'#e5c07b',
	'#ff7a90',
];

export const paletteColor = (index: number): string =>
	PALETTE[index % PALETTE.length];

const channel = (hex: string, i: number) =>
	parseInt(hex.slice(1 + i * 2, 3 + i * 2), 16);

export const mixHex = (hex: string, target: string, t: number): string => {
	const out = [0, 1, 2].map((i) => {
		const v = Math.round(channel(hex, i) * (1 - t) + channel(target, i) * t);
		return v.toString(16).padStart(2, '0');
	});
	return `#${out.join('')}`;
};

export const rgba = (hex: string, alpha: number): string =>
	`rgba(${channel(hex, 0)},${channel(hex, 1)},${channel(hex, 2)},${alpha})`;

export const tone = (hex: string, dark: boolean): string =>
	dark ? hex : mixHex(hex, '#000000', 0.22);

export const labelTone = (hex: string, dark: boolean): string =>
	dark ? mixHex(hex, '#ffffff', 0.38) : mixHex(hex, '#000000', 0.42);

export interface MapCluster {
	key: string;
	title: string;
	color: string;
	count: number;
	hub: number;
	categoryId: number | null;
	isPages: boolean;
}

export interface PreparedMap {
	nodes: GraphNode[];
	index: Map<string, number>;
	cluster: Int32Array;
	degree: Float32Array;
	createdMs: Float64Array;
	radius: Float32Array;
	orphan: Uint8Array;
	hub: Uint8Array;
	edges: GraphEdge[];
	edgeS: Int32Array;
	edgeT: Int32Array;
	edgeCross: Uint8Array;
	adjStart: Int32Array;
	adjEdge: Int32Array;
	clusters: MapCluster[];
	bottleneck: number;
	bottleneckBlocks: number;
	bottleneckEdges: number[];
	labels: string[];
	haystack: string[];
	hubIds: string[];
	map: GraphMap;
}

export const clusterKeyOf = (node: GraphNode): string => {
	if (node.type === 'page') return PAGES_CLUSTER;
	return node.category_id == null ? NONE_CLUSTER : `c${node.category_id}`;
};

export const stripKey = (title: string, key: string | null): string => {
	const text = (title ?? '').trim();
	if (!key || !text.startsWith(key)) return text;
	return text.slice(key.length).replace(/^[\s:–—-]+/, '');
};

export const nodeLabel = (node: GraphNode, max = 26): string => {
	if (node.type !== 'task' || !node.key) return truncateLabel(node.title, max);
	const rest = stripKey(node.title, node.key);
	return truncateLabel(rest ? `${node.key} ${rest}` : node.key, max);
};

export const nodeRadius = (degree: number, hub: boolean): number =>
	hub
		? Math.min(20, 5 + 2.1 * Math.sqrt(degree))
		: Math.min(9, 2.6 + 1.25 * Math.sqrt(degree));

export const prepareMap = (
	map: GraphMap,
	range: { from: string | null; to: string },
): PreparedMap => {
	const fromMs = range.from ? startOfDayMs(range.from) : null;
	const toMs = endOfDayMs(range.to);
	const kept = map.nodes.filter(
		(n) =>
			(n.type === 'task' || n.type === 'page') &&
			inRange(
				parseMs(n.meta?.created_at),
				parseMs(n.meta?.updated_at),
				fromMs,
				toMs,
			),
	);
	const count = kept.length;
	const index = new Map<string, number>();
	kept.forEach((n, i) => index.set(n.id, i));

	const edges = map.edges.filter(
		(e) => index.has(e.from) && index.has(e.to) && e.from !== e.to,
	);
	const edgeS = new Int32Array(edges.length);
	const edgeT = new Int32Array(edges.length);
	const computed = new Float32Array(count);
	edges.forEach((e, i) => {
		edgeS[i] = index.get(e.from)!;
		edgeT[i] = index.get(e.to)!;
		computed[edgeS[i]]++;
		computed[edgeT[i]]++;
	});

	const orphanIds = new Set(map.insights?.orphans?.ids ?? []);
	const degree = new Float32Array(count);
	const orphan = new Uint8Array(count);
	const createdMs = new Float64Array(count);
	kept.forEach((n, i) => {
		const meta = n.meta?.degree;
		const d = typeof meta === 'number' ? meta : computed[i];
		degree[i] = Math.max(d, computed[i]);
		orphan[i] =
			orphanIds.has(n.id) || (typeof meta === 'number' ? meta === 0 : d === 0)
				? 1
				: 0;
		createdMs[i] = parseMs(n.meta?.created_at);
	});

	const titles = new Map<string, string>();
	for (const c of map.categories ?? []) {
		titles.set(c.id == null ? PAGES_CLUSTER : `c${c.id}`, c.title);
	}
	const tally = new Map<string, number>();
	const keys = kept.map(clusterKeyOf);
	keys.forEach((k) => tally.set(k, (tally.get(k) ?? 0) + 1));
	const ordered = [...tally.entries()].sort(
		(a, b) => b[1] - a[1] || a[0].localeCompare(b[0]),
	);
	let colorStep = 0;
	const clusters: MapCluster[] = ordered.map(([key, n]) => {
		const isPages = key === PAGES_CLUSTER;
		const sample = kept[keys.indexOf(key)];
		const title =
			key === PAGES_CLUSTER
				? PAGES_TITLE
				: key === NONE_CLUSTER
				? NONE_TITLE
				: titles.get(key) ?? sample.category ?? 'Category';
		return {
			key,
			title,
			color: isPages ? PAGES_COLOR : paletteColor(colorStep++),
			count: n,
			hub: -1,
			categoryId: isPages || key === NONE_CLUSTER ? null : sample.category_id,
			isPages,
		};
	});
	const clusterIndex = new Map(clusters.map((c, i) => [c.key, i]));
	const cluster = new Int32Array(count);
	keys.forEach((k, i) => {
		cluster[i] = clusterIndex.get(k)!;
	});

	const edgeCross = new Uint8Array(edges.length);
	edges.forEach((_, i) => {
		edgeCross[i] = cluster[edgeS[i]] !== cluster[edgeT[i]] ? 1 : 0;
	});

	const adjStart = new Int32Array(count + 1);
	for (let i = 0; i < edges.length; i++) {
		adjStart[edgeS[i] + 1]++;
		adjStart[edgeT[i] + 1]++;
	}
	for (let i = 0; i < count; i++) adjStart[i + 1] += adjStart[i];
	const fill = adjStart.slice(0, count);
	const adjEdge = new Int32Array(edges.length * 2);
	for (let i = 0; i < edges.length; i++) {
		adjEdge[fill[edgeS[i]]++] = i;
		adjEdge[fill[edgeT[i]]++] = i;
	}

	const best = new Map<number, number>();
	for (let i = 0; i < count; i++) {
		const c = cluster[i];
		const cur = best.get(c);
		if (cur === undefined || degree[i] > degree[cur]) best.set(c, i);
	}
	const hub = new Uint8Array(count);
	best.forEach((i, c) => {
		clusters[c].hub = degree[i] >= 2 ? i : -1;
		if (degree[i] >= 3) hub[i] = 1;
	});
	const hubIds: string[] = [];
	for (const h of map.insights?.hubs ?? []) {
		const i = index.get(h.node.id);
		if (i !== undefined) {
			hub[i] = 1;
			hubIds.push(h.node.id);
		}
	}

	let bottleneck = -1;
	let bottleneckBlocks = 0;
	const bottleneckEdges: number[] = [];
	const top = (map.insights?.bottlenecks ?? []).find(
		(b) => b.blocks > 0 && index.has(b.node.id),
	);
	if (top) {
		bottleneck = index.get(top.node.id)!;
		bottleneckBlocks = top.blocks;
		for (let a = adjStart[bottleneck]; a < adjStart[bottleneck + 1]; a++) {
			const e = adjEdge[a];
			const blocks =
				(edges[e].type === 'blocks' && edgeS[e] === bottleneck) ||
				(edges[e].type === 'depends_on' && edgeT[e] === bottleneck);
			if (blocks) bottleneckEdges.push(e);
		}
	}

	const radius = new Float32Array(count);
	const labels: string[] = [];
	const haystack: string[] = [];
	kept.forEach((n, i) => {
		radius[i] = nodeRadius(degree[i], hub[i] === 1 || i === bottleneck);
		labels.push(nodeLabel(n));
		haystack.push(
			`${n.key ?? ''} ${stripKey(n.title, n.key)} ${
				clusters[cluster[i]].title
			}`.toLowerCase(),
		);
	});

	return {
		nodes: kept,
		index,
		cluster,
		degree,
		createdMs,
		radius,
		orphan,
		hub,
		edges,
		edgeS,
		edgeT,
		edgeCross,
		adjStart,
		adjEdge,
		clusters,
		bottleneck,
		bottleneckBlocks,
		bottleneckEdges,
		labels,
		haystack,
		hubIds,
		map,
	};
};

export interface ClusterPlacement {
	x: number;
	y: number;
	r: number;
}

export const clusterRadius = (count: number): number =>
	48 + 15 * Math.sqrt(Math.max(count, 1));

export const placeClusters = (counts: number[]): ClusterPlacement[] => {
	const n = counts.length;
	if (!n) return [];
	const radii = counts.map(clusterRadius);
	const mean = radii.reduce((a, b) => a + b, 0) / n;
	const golden = 2.399963229728653;
	const out = radii.map((r, i) => {
		const d = n === 1 ? 0 : mean * 1.35 * Math.sqrt(i + 0.6);
		return { x: Math.cos(i * golden) * d, y: Math.sin(i * golden) * d, r };
	});
	const gap = 36;
	for (let pass = 0; pass < 120; pass++) {
		let moved = false;
		for (let i = 0; i < n; i++) {
			for (let j = i + 1; j < n; j++) {
				const dx = out[j].x - out[i].x;
				const dy = out[j].y - out[i].y;
				const dist = Math.hypot(dx, dy) || 0.01;
				const need = out[i].r + out[j].r + gap;
				if (dist >= need) continue;
				const push = (need - dist) / 2;
				const ux = dx / dist;
				const uy = dy / dist;
				out[i].x -= ux * push;
				out[i].y -= uy * push;
				out[j].x += ux * push;
				out[j].y += uy * push;
				moved = true;
			}
		}
		if (!moved) break;
	}
	let wx = 0;
	let wy = 0;
	let wt = 0;
	out.forEach((p) => {
		wx += p.x * p.r;
		wy += p.y * p.r;
		wt += p.r;
	});
	out.forEach((p) => {
		p.x -= wx / wt;
		p.y -= wy / wt;
	});
	return out;
};

export const orphanRingRadius = (placements: ClusterPlacement[]): number =>
	placements.reduce((m, p) => Math.max(m, Math.hypot(p.x, p.y) + p.r), 0) + 50;

export const buildLayoutInit = (
	prepared: PreparedMap,
	group: boolean,
): LayoutInit => {
	const count = prepared.nodes.length;
	const places = placeClusters(prepared.clusters.map((c) => c.count));
	const centers = new Float32Array(places.length * 2);
	const radii = new Float32Array(places.length);
	places.forEach((p, i) => {
		centers[i * 2] = p.x;
		centers[i * 2 + 1] = p.y;
		radii[i] = p.r;
	});
	const links = new Int32Array(prepared.edges.length * 2);
	for (let i = 0; i < prepared.edges.length; i++) {
		links[i * 2] = prepared.edgeS[i];
		links[i * 2 + 1] = prepared.edgeT[i];
	}
	const seed = new Float32Array(count);
	for (let i = 0; i < count; i++) {
		seed[i] = (hashString(prepared.nodes[i].id) % 10000) / 10000;
	}
	return {
		count,
		cluster: prepared.cluster.slice(),
		radius: prepared.radius.slice(),
		orphan: prepared.orphan.slice(),
		links,
		cross: prepared.edgeCross.slice(),
		centers,
		clusterRadius: radii,
		ring: orphanRingRadius(places),
		seed,
		group,
	};
};

export const matchNodes = (
	prepared: PreparedMap,
	query: string,
): number[] | null => {
	const tokens = query.toLowerCase().split(/\s+/).filter(Boolean);
	if (!tokens.length) return null;
	const out: number[] = [];
	for (let i = 0; i < prepared.haystack.length; i++) {
		if (tokens.every((t) => prepared.haystack[i].includes(t))) out.push(i);
	}
	const exact = (i: number) =>
		prepared.nodes[i].key?.toLowerCase() === tokens[0] ? 1 : 0;
	return out.sort(
		(a, b) =>
			exact(b) - exact(a) || prepared.degree[b] - prepared.degree[a] || a - b,
	);
};

export type LabelLevel = 'hubs' | 'linked' | 'all';

export const labelLevel = (zoomRatio: number): LabelLevel =>
	zoomRatio >= 2.4 ? 'all' : zoomRatio >= 1.5 ? 'linked' : 'hubs';

export interface LabelFlags {
	hub: boolean;
	bottleneck: boolean;
	hovered: boolean;
	matched: boolean;
	degree: number;
}

export const shouldLabel = (level: LabelLevel, f: LabelFlags): boolean => {
	if (f.hovered || f.bottleneck || f.matched || f.hub) return true;
	if (level === 'all') return true;
	return level === 'linked' && f.degree >= 3;
};

export const listJoin = (items: string[]): string => {
	if (items.length <= 1) return items.join('');
	return `${items.slice(0, -1).join(', ')} and ${items[items.length - 1]}`;
};

export interface Insight {
	text: string;
	ids: string[];
}

export interface MapInsights {
	bottleneck: Insight | null;
	hubs: Insight | null;
	bridges: Insight | null;
	orphans: Insight;
}

const plural = (n: number, one: string, many: string) =>
	`${n} ${n === 1 ? one : many}`;

export const bridgePairs = (
	prepared: PreparedMap,
): { a: number; b: number; edges: number }[] => {
	const tally = new Map<string, { a: number; b: number; edges: number }>();
	const add = (a: number, b: number, edges: number) => {
		if (a === b || a < 0 || b < 0) return;
		const lo = Math.min(a, b);
		const hi = Math.max(a, b);
		const k = `${lo}:${hi}`;
		const cur = tally.get(k);
		if (cur) cur.edges += edges;
		else tally.set(k, { a: lo, b: hi, edges });
	};
	const resolve = (categoryId: number | null) => {
		if (categoryId == null) {
			const pages = prepared.clusters.findIndex((c) => c.isPages);
			return pages >= 0
				? pages
				: prepared.clusters.findIndex((c) => c.key === NONE_CLUSTER);
		}
		return prepared.clusters.findIndex((c) => c.categoryId === categoryId);
	};
	const given = prepared.map.insights?.bridges ?? [];
	for (const b of given) {
		add(resolve(b.from_category_id), resolve(b.to_category_id), b.edges);
	}
	if (!tally.size) {
		for (let i = 0; i < prepared.edges.length; i++) {
			if (prepared.edgeCross[i]) {
				add(
					prepared.cluster[prepared.edgeS[i]],
					prepared.cluster[prepared.edgeT[i]],
					1,
				);
			}
		}
	}
	return [...tally.values()].sort((x, y) => y.edges - x.edges);
};

export const buildInsights = (prepared: PreparedMap): MapInsights => {
	const { nodes, clusters, cluster } = prepared;

	let bottleneck: Insight | null = null;
	if (prepared.bottleneck >= 0) {
		const b = prepared.bottleneck;
		const blocked = prepared.bottleneckEdges.map((e) =>
			prepared.edgeS[e] === b ? prepared.edgeT[e] : prepared.edgeS[e],
		);
		const tally = new Map<string, number>();
		for (const i of blocked) {
			const c = clusters[cluster[i]];
			if (c.isPages) continue;
			tally.set(c.title, (tally.get(c.title) ?? 0) + 1);
		}
		const names = [...tally.entries()]
			.sort((x, y) => y[1] - x[1] || x[0].localeCompare(y[0]))
			.slice(0, 3)
			.map(([name]) => name);
		const who = nodes[b].key ?? nodeLabel(nodes[b]);
		const base = `${who} blocks ${plural(
			prepared.bottleneckBlocks,
			'task',
			'tasks',
		)}`;
		bottleneck = {
			text: names.length ? `${base} in ${listJoin(names)}` : base,
			ids: [nodes[b].id, ...blocked.map((i) => nodes[i].id)],
		};
	}

	let hubs: Insight | null = null;
	const hubNodes = prepared.hubIds.slice(0, 2).map((id) => {
		const n = nodes[prepared.index.get(id)!];
		const rest = stripKey(n.title, n.key);
		return n.key ? `${n.key} ${truncateLabel(rest, 22)}`.trim() : nodeLabel(n);
	});
	if (hubNodes.length) {
		hubs = {
			text: `${listJoin(hubNodes)} ${
				hubNodes.length === 1 ? 'ties' : 'tie'
			} the most items together`,
			ids: prepared.hubIds.slice(0, 2),
		};
	}

	let bridges: Insight | null = null;
	const pairs = bridgePairs(prepared).slice(0, 8);
	if (pairs.length) {
		const score = new Map<number, number>();
		for (const p of pairs) {
			score.set(p.a, (score.get(p.a) ?? 0) + p.edges);
			score.set(p.b, (score.get(p.b) ?? 0) + p.edges);
		}
		const connector = [...score.entries()].sort(
			(x, y) => y[1] - x[1] || x[0] - y[0],
		)[0][0];
		const partners = pairs
			.filter((p) => p.a === connector || p.b === connector)
			.map((p) => (p.a === connector ? p.b : p.a))
			.slice(0, 3);
		const partnerSet = new Set(partners);
		const ids = new Set<string>();
		for (let i = 0; i < prepared.edges.length && ids.size < 400; i++) {
			if (!prepared.edgeCross[i]) continue;
			const ca = cluster[prepared.edgeS[i]];
			const cb = cluster[prepared.edgeT[i]];
			if (
				(ca === connector && partnerSet.has(cb)) ||
				(cb === connector && partnerSet.has(ca))
			) {
				ids.add(nodes[prepared.edgeS[i]].id);
				ids.add(nodes[prepared.edgeT[i]].id);
			}
		}
		bridges = {
			text: `${clusters[connector].title} connects ${listJoin(
				partners.map((p) => clusters[p].title),
			)}`,
			ids: [...ids],
		};
	}

	const orphanIds: string[] = [];
	for (let i = 0; i < nodes.length; i++) {
		if (prepared.orphan[i]) orphanIds.push(nodes[i].id);
	}
	const total = Math.max(
		prepared.map.insights?.orphans?.total ?? 0,
		orphanIds.length,
	);
	const orphans: Insight = {
		text: total
			? `${plural(
					total,
					'item',
					'items',
			  )} — pages nobody links to, tasks with no relations`
			: 'Every item has at least one link',
		ids: orphanIds,
	};

	return { bottleneck, hubs, bridges, orphans };
};

export const truncationNotice = (
	shown: number,
	total: number,
	truncated: boolean,
): string | null =>
	truncated || total > shown
		? `Showing ${shown} of ${total} — narrow the time range`
		: null;
