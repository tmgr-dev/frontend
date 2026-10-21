import type { TaskCost } from '@/actions/tmgr/taskCost';
import {
	pickUsage,
	ratesRows,
	serverAggregate,
	unpricedWarning,
} from '../serverUsage';
import { buildTree, contextSummary } from '../taskModel';
import type { TaskUsageAggregate } from '../taskUsage';

type Ai = TaskCost['ai'];

const tokens = {
	input: 10,
	output: 20,
	cacheRead: 300,
	cacheWrite5m: 4,
	cacheWrite1h: 2,
	reasoning: 1,
};

const ai = (overrides: Partial<Ai> = {}): Ai => ({
	synced: true,
	sources: 1,
	activeMs: 0,
	lastGeneratedAt: null,
	treeTruncated: false,
	costUsd: '1.500000',
	tokens,
	events: 5,
	unpricedEvents: 0,
	tree: [],
	tools: [],
	...overrides,
});

const localAggregate = (
	cost: number,
	unpriced = 0,
	events = 4,
): TaskUsageAggregate =>
	({
		totals: {
			cost_usd: cost,
			events,
			unpriced_events: unpriced,
			tokens: {
				input: 1,
				output: 1,
				cache_read: 0,
				cache_write_5m: 0,
				cache_write_1h: 0,
				reasoning: 0,
			},
		},
		nodes: [],
		context: { tools: [], mcp: [], hooks: [] },
		truncated: { nodes: false, lists: false },
	} as unknown as TaskUsageAggregate);

describe('serverAggregate', () => {
	it('is null when nothing is synced', () => {
		expect(serverAggregate(null)).toBeNull();
		expect(serverAggregate(undefined)).toBeNull();
		expect(serverAggregate(ai({ synced: false }))).toBeNull();
	});

	it('maps totals, tokens and tools into the local aggregate shape', () => {
		const aggregate = serverAggregate(
			ai({ tools: [{ name: 'Bash', count: 3, errors: 1 }] }),
		);
		expect(aggregate?.totals.cost_usd).toBe(1.5);
		expect(aggregate?.totals.events).toBe(5);
		expect(aggregate?.totals.unpriced_events).toBe(0);
		expect(aggregate?.totals.tokens).toEqual({
			input: 10,
			output: 20,
			cache_read: 300,
			cache_write_5m: 4,
			cache_write_1h: 2,
			reasoning: 1,
		});
		expect(aggregate?.context.tools).toEqual([
			{ name: 'Bash', count: 3, errors: 1, duration_ms: 0 },
		]);
		expect(aggregate?.context.mcp).toEqual([]);
		expect(contextSummary(aggregate!.context)).toEqual({});
	});

	it('maps camelCase tree nodes to TaskNode and keeps the parent link', () => {
		const aggregate = serverAggregate(
			ai({
				tree: [
					{
						id: 'a',
						parentId: null,
						depth: 0,
						tool: 'claude',
						agentType: 'main',
						model: 'claude-opus-4-7',
						durationMs: 5000,
						costUsd: '0.250000',
						events: 2,
						startedAt: '2026-10-01T10:00:00Z',
						activeMs: 3000,
						role: 'lead',
						tokens,
						tools: [{ name: 'Read', count: 2, errors: 0 }],
					},
					{
						id: 'b',
						parentId: 'a',
						tool: 'claude',
						agentType: 'Explore',
						model: 'claude-haiku-4-5',
						tokens,
					},
				],
			}),
		);
		const nodes = aggregate!.nodes;
		expect(nodes[0]).toMatchObject({
			id: 'a',
			parent_id: null,
			agent_type: 'main',
			duration_ms: 5000,
			cost_usd: 0.25,
			active_ms: 3000,
			role: 'lead',
			started_at: '2026-10-01T10:00:00Z',
			events: 2,
		});
		expect(nodes[0].tokens.cache_read).toBe(300);
		expect(nodes[0].tools).toEqual([
			{ name: 'Read', count: 2, errors: 0, duration_ms: 0 },
		]);
		expect(nodes[1]).toMatchObject({
			parent_id: 'a',
			duration_ms: 0,
			cost_usd: 0,
			started_at: '',
			link: null,
			origin: null,
		});
		const tree = buildTree(nodes);
		expect(tree).toHaveLength(1);
		expect(tree[0].children[0].node.id).toBe('b');
	});

	it('is not synced when the aggregate has no tree and no tokens', () => {
		const empty = {
			...tokens,
			input: 0,
			output: 0,
			cacheRead: 0,
			cacheWrite5m: 0,
			cacheWrite1h: 0,
			reasoning: 0,
		};
		expect(serverAggregate(ai({ tokens: empty, tree: [] }))).toBeNull();
		expect(serverAggregate(ai({ tokens: null, tree: null }))).toBeNull();
		expect(
			serverAggregate(ai({ tokens: empty, tree: [{ id: 'a' } as any] })),
		).not.toBeNull();
	});

	it('flags a capped tree', () => {
		expect(serverAggregate(ai({ treeTruncated: true }))?.truncated.nodes).toBe(
			true,
		);
	});

	it('treats an unparsable cost as zero and tolerates missing tokens', () => {
		const aggregate = serverAggregate(
			ai({ costUsd: null, tokens: null, tree: [{ id: 'a' } as any] }),
		);
		expect(aggregate?.totals.cost_usd).toBe(0);
		expect(aggregate?.totals.tokens.input).toBe(0);
	});
});

describe('pickUsage', () => {
	it('returns none when neither side has usage', () => {
		expect(pickUsage({ synced: null, local: null })).toEqual({
			source: 'none',
			aggregate: null,
			notice: null,
		});
	});

	it('shows synced data without notice when no local usage exists', () => {
		const synced = serverAggregate(ai())!;
		const picked = pickUsage({ synced, local: null });
		expect(picked.source).toBe('synced');
		expect(picked.aggregate).toBe(synced);
		expect(picked.notice).toBeNull();
	});

	it('shows local data with a not-synced notice when the server has none', () => {
		const local = localAggregate(2);
		const picked = pickUsage({ synced: null, local });
		expect(picked.source).toBe('local');
		expect(picked.aggregate).toBe(local);
		expect(picked.notice).toContain('Local only');
		expect(picked.notice).toContain('tmgr usage sync');
	});

	it('shows synced data and notes the delta when local is higher by more than a cent', () => {
		const synced = serverAggregate(ai({ costUsd: '1.50' }))!;
		const picked = pickUsage({ synced, local: localAggregate(3.75) });
		expect(picked.source).toBe('synced');
		expect(picked.notice).toBe(
			'This device has $2.25 of usage not yet synced.',
		);
	});

	it('stays quiet when local is within a cent of synced or lower', () => {
		const synced = serverAggregate(ai({ costUsd: '1.50' }))!;
		expect(
			pickUsage({ synced, local: localAggregate(1.51) }).notice,
		).toBeNull();
		expect(pickUsage({ synced, local: localAggregate(1.0) }).notice).toBeNull();
	});

	it('ignores a local aggregate that has no priced cost', () => {
		const synced = serverAggregate(ai())!;
		const picked = pickUsage({
			synced,
			local: localAggregate(9, 4, 4),
		});
		expect(picked.notice).toBeNull();
	});

	it('shows no not-synced notice while the server answer is unknown', () => {
		const local = localAggregate(2);
		expect(pickUsage({ synced: null, local, serverState: 'loading' })).toEqual({
			source: 'local',
			aggregate: local,
			notice: null,
		});
	});

	it('shows a neutral notice when the server request failed', () => {
		const local = localAggregate(2);
		expect(
			pickUsage({ synced: null, local, serverState: 'failed' }).notice,
		).toBe('Could not read the synced usage.');
		expect(
			pickUsage({ synced: null, local: null, serverState: 'failed' }),
		).toEqual({
			source: 'none',
			aggregate: null,
			notice: 'Could not read the synced usage.',
		});
	});

	it('treats a local aggregate with no events as absent', () => {
		expect(
			pickUsage({ synced: null, local: localAggregate(0, 0, 0) }).source,
		).toBe('none');
	});
});

describe('ratesRows', () => {
	it('is empty without rates', () => {
		expect(ratesRows(null)).toEqual([]);
		expect(ratesRows(undefined)).toEqual([]);
	});

	it('formats per-model rates per 1M tokens', () => {
		const rows = ratesRows([
			{
				model: 'claude-opus-4-7',
				input: '15.000000',
				output: '75',
				cacheRead: '1.5',
				cacheWrite5m: '18.75',
				cacheWrite1h: '30',
			},
		]);
		expect(rows).toEqual([
			{
				model: 'opus-4-7',
				text: 'input $15.00 · output $75.00 · cache read $1.50 · cache write 5m $18.75 · cache write 1h $30.00 per 1M tokens',
			},
		]);
	});

	it('keeps sub-cent precision and skips unparsable fields', () => {
		const rows = ratesRows([
			{
				model: 'm',
				input: '0.0625',
				output: 'x',
				cacheRead: '0.3',
				cacheWrite5m: '',
				cacheWrite1h: '',
			},
		]);
		expect(rows[0].text).toBe('input $0.0625 · cache read $0.30 per 1M tokens');
	});
});

describe('unpricedWarning', () => {
	it('is null for zero or missing counts', () => {
		expect(unpricedWarning(0)).toBeNull();
		expect(unpricedWarning(null)).toBeNull();
		expect(unpricedWarning(undefined)).toBeNull();
	});

	it('warns that the cost is understated', () => {
		expect(unpricedWarning(3)).toBe(
			'3 events have no price; the cost is understated.',
		);
		expect(unpricedWarning(1)).toBe(
			'1 event has no price; the cost is understated.',
		);
	});
});
