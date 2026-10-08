import type { GraphInclude } from '@/actions/tmgr/graphParams';
import type {
	GraphEdge,
	GraphNode,
	GraphNodeType,
	GraphResult,
} from '@/types/graph';

export type GraphShape = 'circle' | 'square' | 'ring' | 'diamond' | 'dot';

export interface GraphTypeStyle {
	label: string;
	shape: GraphShape;
	dark: string;
	light: string;
}

export const TYPE_STYLES: Record<GraphNodeType, GraphTypeStyle> = {
	task: { label: 'Task', shape: 'circle', dark: '#5b8cff', light: '#3768e6' },
	page: { label: 'Page', shape: 'square', dark: '#f0a646', light: '#d2831a' },
	user: { label: 'Person', shape: 'ring', dark: '#3ccfb6', light: '#12a38c' },
	persona: {
		label: 'Persona',
		shape: 'diamond',
		dark: '#b493ff',
		light: '#8559e8',
	},
	agent_run: {
		label: 'Agent run',
		shape: 'diamond',
		dark: '#f472b6',
		light: '#dc3f93',
	},
	comment: {
		label: 'Comment',
		shape: 'dot',
		dark: '#8b8b94',
		light: '#71717a',
	},
};

export const isExpandable = (type: GraphNodeType): boolean =>
	type === 'task' || type === 'page';

export const LEGEND_TYPES: GraphNodeType[] = [
	'task',
	'page',
	'user',
	'persona',
	'agent_run',
];

export type GraphGroupKey = 'tasks' | 'pages' | 'people' | 'agent_runs';

export interface GraphGroup {
	key: GraphGroupKey;
	label: string;
	types: GraphNodeType[];
	include: GraphInclude[];
}

export const GRAPH_GROUPS: GraphGroup[] = [
	{ key: 'tasks', label: 'Tasks', types: ['task'], include: ['tasks'] },
	{ key: 'pages', label: 'Pages', types: ['page'], include: ['pages'] },
	{
		key: 'people',
		label: 'People & personas',
		types: ['user', 'persona'],
		include: ['people', 'personas'],
	},
	{
		key: 'agent_runs',
		label: 'Agent runs',
		types: ['agent_run'],
		include: ['agent_runs'],
	},
];

export const ALL_GROUP_KEYS: GraphGroupKey[] = GRAPH_GROUPS.map((g) => g.key);

export const includeFor = (enabled: GraphGroupKey[]): GraphInclude[] =>
	GRAPH_GROUPS.filter((g) => enabled.includes(g.key)).flatMap((g) => g.include);

export const isGroupEnabled = (
	type: GraphNodeType,
	enabled: GraphGroupKey[],
): boolean => {
	if (type === 'comment') return true;
	const group = GRAPH_GROUPS.find((g) => g.types.includes(type));
	return !group || enabled.includes(group.key);
};

export const filterGraph = (
	result: GraphResult,
	enabled: GraphGroupKey[],
): GraphResult => {
	const kept = new Set(
		result.nodes
			.filter((n) => n.id === result.center || isGroupEnabled(n.type, enabled))
			.map((n) => n.id),
	);
	const edges = result.edges.filter((e) => kept.has(e.from) && kept.has(e.to));
	const adjacency = new Map<string, string[]>();
	for (const e of edges) {
		(adjacency.get(e.from) ?? adjacency.set(e.from, []).get(e.from)!).push(
			e.to,
		);
		(adjacency.get(e.to) ?? adjacency.set(e.to, []).get(e.to)!).push(e.from);
	}
	const reachable = new Set<string>([result.center]);
	const queue = [result.center];
	while (queue.length) {
		const id = queue.shift()!;
		for (const next of adjacency.get(id) ?? []) {
			if (!reachable.has(next)) {
				reachable.add(next);
				queue.push(next);
			}
		}
	}
	return {
		...result,
		nodes: result.nodes.filter((n) => reachable.has(n.id)),
		edges: edges.filter((e) => reachable.has(e.from) && reachable.has(e.to)),
	};
};

export const RING_1 = 190;
export const RING_2 = 335;

export interface LayoutShape {
	ax: number;
	ay: number;
}

export const layoutShape = (width: number, height: number): LayoutShape => {
	if (!width || !height) return { ax: 1, ay: 1 };
	const aspect = width / height;
	return { ax: Math.min(1.9, Math.max(1, aspect * 0.85)), ay: 1 };
};

export const ringRadius = (hop: number): number =>
	hop <= 0 ? 0 : hop === 1 ? RING_1 : RING_2;

export const ringTarget = (
	x: number,
	y: number,
	hop: number,
	shape: LayoutShape,
): { x: number; y: number } => {
	const r = ringRadius(hop);
	if (r === 0) return { x: 0, y: 0 };
	const angle = Math.atan2(y / shape.ay, x / shape.ax);
	return {
		x: Math.cos(angle) * r * shape.ax,
		y: Math.sin(angle) * r * shape.ay,
	};
};

export const linkDistance = (
	fromHop: number,
	toHop: number,
	weight: number,
): number => {
	const lo = Math.min(fromHop, toHop);
	const hi = Math.max(fromHop, toHop);
	if (lo === 0) return RING_1 - (Math.min(weight, 3) - 1) * 14;
	if (hi === 2 && lo === 1) return (RING_2 - RING_1) * 0.95;
	return 120;
};

export const nodeRadius = (node: GraphNode, compact: boolean): number => {
	const k = compact ? 1.3 : 1;
	if (node.hop === 0) return 34 * k;
	if (node.type === 'comment') return 6 * k;
	const base = node.hop === 1 ? 16 + Math.min(node.weight, 3) * 1.4 : 11;
	return base * k;
};

export const hashString = (value: string): number => {
	let h = 2166136261;
	for (let i = 0; i < value.length; i++) {
		h ^= value.charCodeAt(i);
		h = Math.imul(h, 16777619);
	}
	return h >>> 0;
};

export const edgeBend = (edgeId: string): number => {
	const h = hashString(edgeId);
	const magnitude = 0.1 + ((h >>> 3) % 8) / 100;
	return (h & 1 ? 1 : -1) * magnitude;
};

export const curveControl = (
	ax: number,
	ay: number,
	bx: number,
	by: number,
	bend: number,
): { x: number; y: number } => {
	const mx = (ax + bx) / 2;
	const my = (ay + by) / 2;
	const dx = bx - ax;
	const dy = by - ay;
	return { x: mx - dy * bend * 2, y: my + dx * bend * 2 };
};

export const quadPoint = (
	ax: number,
	ay: number,
	cx: number,
	cy: number,
	bx: number,
	by: number,
	t: number,
): { x: number; y: number } => {
	const u = 1 - t;
	return {
		x: u * u * ax + 2 * u * t * cx + t * t * bx,
		y: u * u * ay + 2 * u * t * cy + t * t * by,
	};
};

export interface Neighbourhood {
	nodes: Set<string>;
	edges: Set<string>;
}

export const neighbourhood = (
	edges: GraphEdge[],
	id: string,
): Neighbourhood => {
	const nodes = new Set<string>([id]);
	const touching = new Set<string>();
	for (const e of edges) {
		if (e.from === id || e.to === id) {
			touching.add(e.id);
			nodes.add(e.from);
			nodes.add(e.to);
		}
	}
	return { nodes, edges: touching };
};

export const whyFor = (result: GraphResult, id: string): string[] =>
	result.edges
		.filter((e) => e.from === id || e.to === id)
		.map((e) => e.why)
		.filter(Boolean);

export interface SeedPoint {
	x: number;
	y: number;
	fresh: boolean;
}

export const seedPositions = (
	previous: Map<string, { x: number; y: number }>,
	nodes: GraphNode[],
	edges: GraphEdge[],
): Map<string, SeedPoint> => {
	const hopOf = new Map(nodes.map((n) => [n.id, n.hop]));
	const parentOf = new Map<string, string>();
	for (const e of edges) {
		const hf = hopOf.get(e.from) ?? 0;
		const ht = hopOf.get(e.to) ?? 0;
		if (hf < ht) parentOf.set(e.to, e.from);
		else if (ht < hf) parentOf.set(e.from, e.to);
	}
	const out = new Map<string, SeedPoint>();
	let fresh = 0;
	for (const node of [...nodes].sort((a, b) => a.hop - b.hop)) {
		const known = previous.get(node.id);
		if (known) {
			out.set(node.id, { x: known.x, y: known.y, fresh: false });
			continue;
		}
		const parent = parentOf.get(node.id);
		const origin = (parent && out.get(parent)) ||
			(parent && previous.get(parent)) || { x: 0, y: 0 };
		const angle = fresh++ * 2.399963;
		const spread = node.hop === 0 ? 0 : 8;
		out.set(node.id, {
			x: origin.x + Math.cos(angle) * spread,
			y: origin.y + Math.sin(angle) * spread,
			fresh: true,
		});
	}
	return out;
};

export interface Bounds {
	minX: number;
	minY: number;
	maxX: number;
	maxY: number;
}

export interface Transform {
	k: number;
	x: number;
	y: number;
}

export const fitTransform = (
	bounds: Bounds,
	width: number,
	height: number,
	padding: number,
	maxK = 1.6,
): Transform => {
	const bw = Math.max(bounds.maxX - bounds.minX, 1);
	const bh = Math.max(bounds.maxY - bounds.minY, 1);
	const k = Math.min(
		maxK,
		Math.max(
			0.05,
			Math.min((width - padding * 2) / bw, (height - padding * 2) / bh),
		),
	);
	const cx = (bounds.minX + bounds.maxX) / 2;
	const cy = (bounds.minY + bounds.maxY) / 2;
	return { k, x: width / 2 - cx * k, y: height / 2 - cy * k };
};

export const truncateLabel = (value: string, max: number): string => {
	const text = (value ?? '').trim();
	return text.length > max ? `${text.slice(0, max - 1).trimEnd()}…` : text;
};

export interface GraphTheme {
	dark: boolean;
	bg: string;
	bgEdge: string;
	ink: string;
	inkMuted: string;
	line: string;
	pill: string;
	halo: string;
	centerRing: string;
	colors: Record<GraphNodeType, string>;
	fontFamily: string;
}

export type VarReader = (name: string) => string;

const pick = (read: VarReader, name: string, fallback: string): string => {
	const value = (read(name) ?? '').trim();
	return value || fallback;
};

export const resolveTheme = (
	read: VarReader,
	dark: boolean,
	fontFamily = 'system-ui, sans-serif',
): GraphTheme => {
	const colors = {} as Record<GraphNodeType, string>;
	(Object.keys(TYPE_STYLES) as GraphNodeType[]).forEach((type) => {
		colors[type] = dark ? TYPE_STYLES[type].dark : TYPE_STYLES[type].light;
	});
	const bg = dark
		? pick(read, '--bg-sunken', '#11141b')
		: pick(read, '--bg-raised', '#ffffff');
	return {
		dark,
		bg,
		bgEdge: dark
			? pick(read, '--bg-sunken', '#0b0d12')
			: pick(read, '--bg-sunken', '#efeff1'),
		ink: pick(read, '--fg', dark ? '#e8eaf0' : '#18181b'),
		inkMuted: pick(read, '--fg-muted', dark ? '#9aa3b5' : '#52525b'),
		line: dark ? 'rgba(255,255,255,0.1)' : 'rgba(24,24,27,0.12)',
		pill: dark ? 'rgba(17,20,27,0.72)' : 'rgba(255,255,255,0.8)',
		halo: bg,
		centerRing: dark ? '#c3d3ff' : '#1d3f9f',
		colors,
		fontFamily,
	};
};

export const readThemeFromDom = (el: Element): GraphTheme => {
	const style = getComputedStyle(document.documentElement);
	const dark = document.documentElement.classList.contains('dark');
	return resolveTheme(
		(name) => style.getPropertyValue(name),
		dark,
		getComputedStyle(el).fontFamily || undefined,
	);
};

export const edgeColorType = (
	edge: GraphEdge,
	types: Map<string, GraphNodeType>,
	center: string,
): GraphNodeType => {
	const a = types.get(edge.from) ?? 'task';
	const b = types.get(edge.to) ?? 'task';
	if (a === 'task' && b === 'task') return 'task';
	if (a === 'task') return b;
	if (b === 'task') return a;
	return edge.from === center ? b : a;
};
