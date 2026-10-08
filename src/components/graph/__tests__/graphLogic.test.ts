import type { GraphEdge, GraphNode, GraphResult } from '@/types/graph';
import {
	curveControl,
	edgeBend,
	filterGraph,
	fitTransform,
	includeFor,
	isExpandable,
	layoutShape,
	linkDistance,
	neighbourhood,
	quadPoint,
	resolveTheme,
	RING_1,
	RING_2,
	ringRadius,
	ringTarget,
	seedPositions,
	truncateLabel,
	whyFor,
} from '../graphLogic';

const node = (id: string, type: GraphNode['type'], hop: number): GraphNode => ({
	id,
	type,
	ref_id: 1,
	key: null,
	title: id,
	status: null,
	category_id: null,
	category: null,
	hop,
	weight: 1,
	rel: null,
	meta: {},
});
const edge = (from: string, to: string, why = ''): GraphEdge => ({
	id: `${from}>${to}`,
	from,
	to,
	type: 'relates_to',
	label: 'relates to',
	weight: 2,
	why,
});

const sample: GraphResult = {
	center: 'task:1',
	nodes: [
		node('task:1', 'task', 0),
		node('task:2', 'task', 1),
		node('page:3', 'page', 1),
		node('user:4', 'user', 1),
		node('task:5', 'task', 2),
		node('page:6', 'page', 2),
	],
	edges: [
		edge('task:1', 'task:2', 'one'),
		edge('page:3', 'task:1', 'two'),
		edge('task:1', 'user:4'),
		edge('task:2', 'task:5'),
		edge('page:3', 'page:6'),
	],
	truncated: false,
	caps_hit: {},
};

describe('layout helpers', () => {
	it('uses fixed ring radii by hop', () => {
		expect(ringRadius(0)).toBe(0);
		expect(ringRadius(1)).toBe(RING_1);
		expect(ringRadius(2)).toBe(RING_2);
	});

	it('projects a node onto its ring along its current angle', () => {
		const t = ringTarget(10, 0, 1, { ax: 1, ay: 1 });
		expect(t.x).toBeCloseTo(RING_1);
		expect(t.y).toBeCloseTo(0);
		const wide = ringTarget(0, 5, 2, { ax: 1.5, ay: 1 });
		expect(wide.y).toBeCloseTo(RING_2);
	});

	it('stretches rings with the aspect ratio but caps it', () => {
		expect(layoutShape(400, 400).ax).toBe(1);
		expect(layoutShape(900, 300).ax).toBeLessThanOrEqual(1.9);
		expect(layoutShape(0, 0)).toEqual({ ax: 1, ay: 1 });
	});

	it('shortens direct links as weight grows', () => {
		expect(linkDistance(0, 1, 3)).toBeLessThan(linkDistance(0, 1, 1));
		expect(linkDistance(1, 2, 1)).toBeLessThan(RING_2);
	});

	it('bends curves deterministically and symmetric around the midpoint', () => {
		expect(edgeBend('a')).toBe(edgeBend('a'));
		const c = curveControl(0, 0, 100, 0, 0.1);
		expect(c.x).toBeCloseTo(50);
		expect(Math.abs(c.y)).toBeCloseTo(20);
		const mid = quadPoint(0, 0, c.x, c.y, 100, 0, 0.5);
		expect(mid.x).toBeCloseTo(50);
		expect(quadPoint(0, 0, c.x, c.y, 100, 0, 0)).toEqual({ x: 0, y: 0 });
	});

	it('fits bounds into the viewport', () => {
		const t = fitTransform(
			{ minX: -100, minY: -50, maxX: 100, maxY: 50 },
			400,
			200,
			20,
		);
		expect(t.k).toBeCloseTo(1.6 > 1.6 ? 1.6 : 1.6);
		expect(t.x).toBeCloseTo(200);
		expect(t.y).toBeCloseTo(100);
	});

	it('truncates labels', () => {
		expect(truncateLabel('short', 10)).toBe('short');
		expect(truncateLabel('a very long title indeed', 10)).toHaveLength(10);
	});
});

describe('include filtering', () => {
	it('maps enabled groups to API include tokens', () => {
		expect(includeFor(['tasks', 'people'])).toEqual([
			'tasks',
			'people',
			'personas',
		]);
	});

	it('drops disabled types, dangling edges and unreachable nodes but keeps the centre', () => {
		const out = filterGraph(sample, ['tasks']);
		expect(out.nodes.map((n) => n.id).sort()).toEqual([
			'task:1',
			'task:2',
			'task:5',
		]);
		expect(out.edges).toHaveLength(2);
	});

	it('drops outer nodes whose only path ran through a hidden node', () => {
		const out = filterGraph(sample, ['tasks', 'people']);
		expect(out.nodes.map((n) => n.id)).not.toContain('page:6');
		expect(out.nodes.map((n) => n.id)).toContain('user:4');
	});

	it('keeps the centre even when its own group is off', () => {
		const out = filterGraph(sample, ['pages']);
		expect(out.nodes.map((n) => n.id)).toContain('task:1');
	});
});

describe('neighbourhood highlight', () => {
	it('returns the node, its neighbours and touching edges', () => {
		const hood = neighbourhood(sample.edges, 'task:2');
		expect([...hood.nodes].sort()).toEqual(['task:1', 'task:2', 'task:5']);
		expect(hood.edges.size).toBe(2);
	});

	it('lists why-sentences for edges touching a node', () => {
		expect(whyFor(sample, 'task:1')).toEqual(['one', 'two']);
	});
});

describe('data merge', () => {
	it('keeps positions of persisting nodes and seeds new ones at their parent', () => {
		const previous = new Map([
			['task:1', { x: 0, y: 0 }],
			['task:2', { x: 120, y: 40 }],
		]);
		const seeds = seedPositions(previous, sample.nodes, sample.edges);
		expect(seeds.get('task:2')).toEqual({ x: 120, y: 40, fresh: false });
		const child = seeds.get('task:5')!;
		expect(child.fresh).toBe(true);
		expect(Math.hypot(child.x - 120, child.y - 40)).toBeLessThanOrEqual(8.01);
	});

	it('falls back to the origin when the parent is unknown', () => {
		const seeds = seedPositions(new Map(), sample.nodes, sample.edges);
		const first = seeds.get('task:2')!;
		expect(Math.hypot(first.x, first.y)).toBeLessThanOrEqual(8.01);
	});
});

describe('theme resolution', () => {
	it('falls back to defaults when tokens are missing', () => {
		const theme = resolveTheme(() => '', true);
		expect(theme.bg).toBe('#11141b');
		expect(theme.colors.task).toBe('#5b8cff');
		const light = resolveTheme(() => '', false);
		expect(light.bg).toBe('#ffffff');
		expect(light.colors.task).not.toBe('#5b8cff');
	});

	it('prefers CSS tokens when present', () => {
		const theme = resolveTheme(
			(name) => (name === '--fg' ? ' #123456 ' : ''),
			true,
		);
		expect(theme.ink).toBe('#123456');
	});
});

describe('expandable types', () => {
	it('only tasks and pages can be re-centred', () => {
		expect(isExpandable('task')).toBe(true);
		expect(isExpandable('page')).toBe(true);
		for (const type of ['user', 'persona', 'agent_run', 'comment'] as const)
			expect(isExpandable(type)).toBe(false);
	});
});
