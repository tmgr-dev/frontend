import type { GraphEdge, GraphMap, GraphNode } from '@/types/graph';
import {
	buildInsights,
	buildLayoutInit,
	clusterRadius,
	DAY_MS,
	inRange,
	labelLevel,
	listJoin,
	matchNodes,
	nodeLabel,
	placeClusters,
	prepareMap,
	rangeBounds,
	shouldLabel,
	timelineSpan,
	timelineSteps,
	truncationNotice,
	visibleAt,
} from '../mapLogic';

const task = (
	id: number,
	key: string,
	categoryId: number | null,
	category: string | null,
	extra: Partial<GraphNode['meta']> = {},
): GraphNode => ({
	id: `task:${id}`,
	type: 'task',
	ref_id: id,
	key,
	title: `${key}: Title ${id}`,
	status: null,
	category_id: categoryId,
	category,
	hop: 0,
	weight: 1,
	rel: null,
	meta: {
		created_at: '2026-09-20T10:00:00Z',
		updated_at: '2026-10-01T10:00:00Z',
		degree: 0,
		blocks: 0,
		...extra,
	},
});
const page = (id: number, title: string): GraphNode => ({
	...task(id, '', null, null),
	id: `page:${id}`,
	type: 'page',
	key: null,
	title,
});
const edge = (from: string, to: string, type = 'relates_to'): GraphEdge => ({
	id: `${type}:${from}:${to}`,
	from,
	to,
	type,
	label: type,
	weight: 2,
	why: '',
});

const nodes = [
	task(1, 'TM-416', 2, 'Desktop'),
	task(2, 'TM-420', 2, 'Desktop'),
	task(3, 'TM-421', 4, 'Agents'),
	task(4, 'TM-422', 4, 'Agents'),
	task(5, 'TM-423', 4, 'Agents'),
	page(6, 'Pages index'),
	page(7, 'Lonely page'),
	task(8, 'TM-430', null, null),
];
const edges = [
	edge('task:1', 'task:2', 'blocks'),
	edge('task:1', 'task:3', 'blocks'),
	edge('task:1', 'task:4', 'blocks'),
	edge('task:5', 'task:1', 'depends_on'),
	edge('task:3', 'task:4'),
	edge('page:6', 'task:3', 'linked_page'),
];
const rank = (n: GraphNode, degree: number, blocks: number) => ({
	node: n,
	degree,
	blocks,
});
const map: GraphMap = {
	nodes,
	edges,
	categories: [
		{ id: 2, title: 'Desktop', code: 'TM', count: 3 },
		{ id: 4, title: 'Agents & personas', code: 'TM', count: 3 },
		{ id: null, title: 'Uncategorized', code: null, count: 1 },
	],
	insights: {
		hubs: [rank(nodes[0], 4, 4), rank(nodes[5], 1, 0)],
		bottlenecks: [rank(nodes[0], 4, 4)],
		bridges: [
			{ from_category_id: 2, to_category_id: 4, edges: 3 },
			{ from_category_id: 4, to_category_id: null, edges: 1 },
		],
		orphans: { total: 2, ids: ['page:7', 'task:8'] },
	},
	truncated: false,
	total: 8,
};
for (const e of edges) {
	for (const id of [e.from, e.to]) {
		const n = nodes.find((x) => x.id === id)!;
		n.meta.degree = (n.meta.degree as number) + 1;
	}
}
const range = { from: '2026-09-01', to: '2026-10-08' };

describe('rangeBounds', () => {
	it('computes from and to for presets', () => {
		const now = new Date(2026, 9, 8, 15, 0, 0);
		expect(rangeBounds('30', now)).toEqual({
			from: '2026-09-08',
			to: '2026-10-08',
		});
		expect(rangeBounds('7', now).from).toBe('2026-10-01');
		expect(rangeBounds('90', now).from).toBe('2026-07-10');
		expect(rangeBounds('all', now)).toEqual({ from: null, to: '2026-10-08' });
	});
});

describe('range filtering', () => {
	const t = (s: string) => Date.parse(s);
	it('keeps items created before the end and updated after the start', () => {
		const from = t('2026-09-10T00:00:00Z');
		const to = t('2026-10-01T00:00:00Z');
		expect(
			inRange(t('2026-09-01T00:00:00Z'), t('2026-09-20T00:00:00Z'), from, to),
		).toBe(true);
		expect(
			inRange(t('2026-09-01T00:00:00Z'), t('2026-09-05T00:00:00Z'), from, to),
		).toBe(false);
		expect(
			inRange(t('2026-10-02T00:00:00Z'), t('2026-10-03T00:00:00Z'), from, to),
		).toBe(false);
		expect(
			inRange(t('2020-01-01T00:00:00Z'), t('2020-01-02T00:00:00Z'), null, to),
		).toBe(true);
	});

	it('drops nodes outside the range when preparing and their edges', () => {
		const old = task(9, 'TM-1', 2, 'Desktop', {
			created_at: '2026-01-01T00:00:00Z',
			updated_at: '2026-01-05T00:00:00Z',
		});
		const prepared = prepareMap(
			{
				...map,
				nodes: [...nodes, old],
				edges: [...edges, edge('task:9', 'task:1')],
			},
			range,
		);
		expect(prepared.index.has('task:9')).toBe(false);
		expect(prepared.edges).toHaveLength(edges.length);
	});
});

describe('timeline', () => {
	it('shows items created up to the cursor', () => {
		expect(visibleAt(100, 100)).toBe(true);
		expect(visibleAt(101, 100)).toBe(false);
		expect(visibleAt(NaN, 100)).toBe(true);
	});

	it('steps weekly for long spans and daily for short ones', () => {
		const end = Date.UTC(2026, 9, 8);
		const month = timelineSteps(end - 30 * DAY_MS, end);
		expect(month[1] - month[0]).toBe(7 * DAY_MS);
		expect(month[month.length - 1]).toBe(end);
		const week = timelineSteps(end - 7 * DAY_MS, end);
		expect(week).toHaveLength(8);
		expect(week[1] - week[0]).toBe(DAY_MS);
	});

	it('starts all time at the oldest item and never inverts', () => {
		const end = Date.UTC(2026, 9, 8);
		expect(
			timelineSpan([end - 5 * DAY_MS, end - 40 * DAY_MS], null, end).start,
		).toBe(end - 40 * DAY_MS);
		expect(timelineSpan([], end + 1, end).start).toBeLessThan(end);
	});
});

describe('cluster placement', () => {
	it('is deterministic and keeps clusters apart', () => {
		const counts = [120, 80, 60, 40, 30, 12, 5];
		const a = placeClusters(counts);
		expect(placeClusters(counts)).toEqual(a);
		for (let i = 0; i < a.length; i++) {
			expect(a[i].r).toBe(clusterRadius(counts[i]));
			for (let j = i + 1; j < a.length; j++) {
				expect(
					Math.hypot(a[i].x - a[j].x, a[i].y - a[j].y),
				).toBeGreaterThanOrEqual(a[i].r + a[j].r);
			}
		}
	});

	it('bounds the placement work for very many clusters', () => {
		const counts = Array.from({ length: 1000 }, (_, i) => 1 + (i % 40));
		const started = Date.now();
		const places = placeClusters(counts);
		expect(places).toHaveLength(1000);
		expect(Date.now() - started).toBeLessThan(400);
	});

	it('handles one and zero clusters', () => {
		expect(placeClusters([])).toEqual([]);
		const [only] = placeClusters([10]);
		expect(Math.hypot(only.x, only.y)).toBeLessThan(1);
	});

	it('builds a layout payload with cluster sizes and links', () => {
		const prepared = prepareMap(map, range);
		const init = buildLayoutInit(prepared, true);
		expect(init.count).toBe(prepared.nodes.length);
		expect(init.links).toHaveLength(prepared.edges.length * 2);
		expect(Array.from(init.clusterCounts)).toEqual(
			prepared.clusters.map((c) => c.count),
		);
	});
});

describe('prepareMap', () => {
	const prepared = prepareMap(map, range);
	it('groups pages into their own cluster and orders clusters by size', () => {
		const titles = prepared.clusters.map((c) => c.title);
		expect(titles).toContain('Pages & docs');
		expect(titles).toContain('Uncategorized');
		expect(titles).toContain('Desktop');
		expect(prepared.clusters.find((c) => c.isPages)!.color).toBe('#f0a646');
		const counts = prepared.clusters.map((c) => c.count);
		expect([...counts].sort((a, b) => b - a)).toEqual(counts);
	});

	it('flags orphans, finds the bottleneck and its blocking edges', () => {
		expect(prepared.orphan[prepared.index.get('page:7')!]).toBe(1);
		expect(prepared.orphan[prepared.index.get('task:1')!]).toBe(0);
		expect(prepared.bottleneck).toBe(prepared.index.get('task:1'));
		expect(prepared.bottleneckEdges).toHaveLength(4);
	});

	it('marks cross-cluster edges', () => {
		const cross = Array.from(prepared.edgeCross).filter(Boolean).length;
		expect(cross).toBe(4);
	});
});

describe('insights', () => {
	const insights = buildInsights(prepareMap(map, range));
	it('names the bottleneck with the categories it blocks', () => {
		expect(insights.bottleneck!.text).toBe(
			'TM-416 blocks 4 tasks in Agents & personas and Desktop',
		);
		expect(insights.bottleneck!.ids[0]).toBe('task:1');
	});

	it('describes hubs and bridges', () => {
		expect(insights.hubs!.text).toBe(
			'TM-416 Title 1 and Pages index tie the most items together',
		);
		expect(insights.bridges!.text).toBe(
			'Agents & personas connect Desktop and Uncategorized',
		);
	});

	it('counts items with no links', () => {
		expect(insights.orphans.text).toBe(
			'2 items — pages nobody links to, tasks with no relations',
		);
		expect(insights.orphans.ids).toEqual(['page:7', 'task:8']);
	});

	it('omits the bottleneck and says so when everything is linked', () => {
		const calm = buildInsights(
			prepareMap(
				{
					...map,
					insights: {
						...map.insights,
						bottlenecks: [],
						orphans: { total: 0, ids: [] },
					},
					nodes: nodes.slice(0, 5).map((n) => ({
						...n,
						meta: { ...n.meta, degree: 1 },
					})),
					edges: [
						edge('task:1', 'task:2'),
						edge('task:3', 'task:4'),
						edge('task:5', 'task:1'),
					],
				},
				range,
			),
		);
		expect(calm.bottleneck).toBeNull();
		expect(calm.orphans.text).toBe('Every item has at least one link');
	});

	it('falls back to computed bridges when the server sends none', () => {
		const computed = buildInsights(
			prepareMap({ ...map, insights: { ...map.insights, bridges: [] } }, range),
		);
		expect(computed.bridges).not.toBeNull();
	});
});

describe('text helpers', () => {
	it('joins lists in plain English', () => {
		expect(listJoin([])).toBe('');
		expect(listJoin(['A'])).toBe('A');
		expect(listJoin(['A', 'B'])).toBe('A and B');
		expect(listJoin(['A', 'B', 'C'])).toBe('A, B and C');
	});

	it('does not repeat the task key in labels', () => {
		expect(nodeLabel(task(1, 'TM-9', 1, 'X'))).toBe('TM-9 Title 1');
		expect(nodeLabel({ ...task(1, 'TM-9', 1, 'X'), title: 'Plain' })).toBe(
			'TM-9 Plain',
		);
	});

	it('words the truncation notice', () => {
		expect(truncationNotice(3000, 5400, true)).toBe(
			'Showing 3000 of 5400 — narrow the time range',
		);
		expect(truncationNotice(8, 8, false)).toBeNull();
	});
});

describe('search matching', () => {
	const prepared = prepareMap(map, range);
	it('matches key, title and category, all tokens required', () => {
		expect(matchNodes(prepared, '')).toBeNull();
		expect(
			matchNodes(prepared, 'tm-416')!.map((i) => prepared.nodes[i].key),
		).toEqual(['TM-416']);
		expect(matchNodes(prepared, 'agents')).toHaveLength(3);
		expect(matchNodes(prepared, 'agents tm-421')).toHaveLength(1);
		expect(matchNodes(prepared, 'nothing like this')).toEqual([]);
	});

	it('ranks an exact key first, then the most connected', () => {
		const hits = matchNodes(prepared, 'desktop')!;
		expect(prepared.nodes[hits[0]].key).toBe('TM-416');
	});
});

describe('label level of detail', () => {
	it('shows more labels as the view zooms in', () => {
		expect(labelLevel(1)).toBe('hubs');
		expect(labelLevel(1.6)).toBe('linked');
		expect(labelLevel(3)).toBe('all');
	});

	it('always labels hubs, the bottleneck, hover and matches', () => {
		const base = {
			hub: false,
			bottleneck: false,
			hovered: false,
			matched: false,
			degree: 0,
		};
		expect(shouldLabel('hubs', base)).toBe(false);
		for (const key of ['hub', 'bottleneck', 'hovered', 'matched'] as const) {
			expect(shouldLabel('hubs', { ...base, [key]: true })).toBe(true);
		}
		expect(shouldLabel('linked', { ...base, degree: 3 })).toBe(true);
		expect(shouldLabel('linked', { ...base, degree: 1 })).toBe(false);
		expect(shouldLabel('all', base)).toBe(true);
	});
});
