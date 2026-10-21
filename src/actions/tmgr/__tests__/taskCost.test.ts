jest.mock('@/plugins/axios', () => ({
	__esModule: true,
	default: { get: jest.fn() },
}));

import $axios from '@/plugins/axios';
import { pickUsage, ratesRows, serverAggregate } from '@/usage/serverUsage';
import { timeRows } from '@/utils/agentWork';
import { camelizeKeys, getTaskCost } from '../taskCost';

const tokens = {
	input: 100,
	output: 200,
	cacheRead: 3000,
	cache_write_5m: 40,
	cache_write_1h: 5,
	reasoning: 6,
};

const wire = {
	task_id: 42,
	human: {
		seconds: 600,
		timer_running: false,
		rate: { value: '60.00', source: 'task', hidden: false },
		cost: '10.00',
	},
	agents: {
		working_seconds: 1200,
		waiting_seconds: 300,
		runs: 2,
		running: 0,
		active_seconds: 700,
		wall_clock_seconds: 1000,
	},
	ai: {
		synced: true,
		sources: 1,
		cost_usd: '1.500000',
		events: 5,
		unpriced_events: 2,
		tokens,
		active_ms: 4000,
		last_generated_at: '2026-10-01T10:00:00Z',
		tree: [
			{
				source: 1,
				id: 'root',
				parent_id: null,
				depth: 0,
				tool: 'claude',
				agent_type: 'main',
				model: 'claude-opus-4-7',
				origin: null,
				link: null,
				duration_ms: 9000,
				cost_usd: '1.000000',
				events: 3,
				started_at: '2026-10-01T10:00:00Z',
				ended_at: '2026-10-01T10:00:09Z',
				active_ms: 4000,
				role: 'lead',
				tokens,
				tools: [{ name: 'Bash', count: 2, errors: 0 }],
			},
			{
				source: 1,
				id: 'child',
				parent_id: 'root',
				depth: 1,
				tool: 'claude',
				agent_type: 'Explore',
				model: 'claude-haiku-4-5',
				origin: 'task',
				link: 'exact',
				duration_ms: 1000,
				cost_usd: '0.500000',
				events: 2,
				tokens,
				tools: [],
			},
		],
		tree_truncated: false,
		tools: [{ name: 'Bash', count: 2, errors: 0 }],
		rates: [
			{
				model: 'claude-opus-4-7',
				input: '15',
				output: '75',
				cache_read: '1.5',
				cache_write_5m: '18.75',
				cache_write_1h: '30',
			},
		],
	},
	totals: { seconds: 1800, cost: '11.50' },
};

beforeEach(() => {
	jest.clearAllMocks();
});

describe('camelizeKeys', () => {
	it('converts nested keys and leaves values alone', () => {
		expect(
			camelizeKeys({
				a_b: [{ c_d_1h: 'keep_me' }],
				e: null,
				cache_write_5m: 1,
			}),
		).toEqual({ aB: [{ cD1h: 'keep_me' }], e: null, cacheWrite5m: 1 });
	});
});

describe('getTaskCost', () => {
	it('camelizes the snake_case wire body, enveloped or not', async () => {
		($axios.get as jest.Mock).mockResolvedValueOnce({ data: { data: wire } });
		const cost = await getTaskCost(42);
		expect(cost?.taskId).toBe(42);
		expect(cost?.human.timerRunning).toBe(false);
		expect(cost?.agents.workingSeconds).toBe(1200);
		expect(cost?.agents.wallClockSeconds).toBe(1000);
		expect(cost?.ai.costUsd).toBe('1.500000');
		expect(cost?.ai.unpricedEvents).toBe(2);
		expect(cost?.ai.tokens?.cacheWrite5m).toBe(40);
		expect(cost?.ai.tree?.[0].parentId).toBeNull();
		expect(cost?.ai.tree?.[0].activeMs).toBe(4000);
		($axios.get as jest.Mock).mockResolvedValueOnce({ data: wire });
		expect((await getTaskCost(42))?.ai.treeTruncated).toBe(false);
	});

	it('is null on failure or an unexpected body', async () => {
		($axios.get as jest.Mock).mockRejectedValueOnce(new Error('x'));
		expect(await getTaskCost(1)).toBeNull();
		($axios.get as jest.Mock).mockResolvedValueOnce({ data: { data: {} } });
		expect(await getTaskCost(1)).toBeNull();
	});

	it('feeds the mapper, source selection, rates and time rows', async () => {
		($axios.get as jest.Mock).mockResolvedValueOnce({ data: { data: wire } });
		const cost = await getTaskCost(42);
		const synced = serverAggregate(cost!.ai)!;
		expect(synced.totals.cost_usd).toBe(1.5);
		expect(synced.totals.unpriced_events).toBe(2);
		expect(synced.totals.tokens.cache_write_5m).toBe(40);
		expect(synced.totals.tokens.cache_read).toBe(3000);
		expect(synced.nodes.map((n) => [n.id, n.parent_id])).toEqual([
			['root', null],
			['child', 'root'],
		]);
		expect(synced.nodes[0]).toMatchObject({
			agent_type: 'main',
			duration_ms: 9000,
			active_ms: 4000,
			role: 'lead',
			cost_usd: 1,
		});
		expect(pickUsage({ synced, local: null }).source).toBe('synced');
		expect(ratesRows(cost!.ai.rates)[0].text).toContain(
			'cache write 5m $18.75',
		);
		expect(timeRows(1200, cost!.agents).map((r) => r.key)).toEqual([
			'active',
			'wall',
			'waiting',
			'agents',
		]);
	});
});
