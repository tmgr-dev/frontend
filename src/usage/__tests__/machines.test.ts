import type { DailyRow, Machine } from '@/actions/tmgr/aiUsageMachines';
import {
	agentName,
	ageText,
	aggregateTotals,
	buildDailySeries,
	countdownText,
	dailyQuery,
	filterOptions,
	groupByModelTask,
	limitRows,
	limitState,
	localDate,
	periodRange,
	rowTokens,
	windowLabel,
} from '../machines';

const row = (over: Partial<DailyRow> = {}): DailyRow => ({
	day: '2026-11-20',
	machine_id: 'm1',
	agent: 'claude-code',
	model: 'opus',
	task_id: null,
	task_key: null,
	workspace_id: null,
	input_tokens: 1,
	output_tokens: 2,
	cache_read_tokens: 3,
	cache_write_tokens: 4,
	reasoning_tokens: 5,
	cost_usd: 1.5,
	sessions: 2,
	events: 10,
	...over,
});

const machine = (id: string, label: string): Machine => ({
	machine_id: id,
	label,
	platform: 'macos',
	cli_version: '1.0.0',
	timezone: null,
	last_seen_at: '2026-11-21T10:00:00Z',
	limits: [],
});

const NOW = new Date('2026-11-21T12:00:00Z');

describe('periodRange', () => {
	it('uses local dates inclusive of today', () => {
		expect(periodRange('today', NOW)).toEqual({
			from: '2026-11-21',
			to: '2026-11-21',
		});
		expect(periodRange('7d', NOW)).toEqual({
			from: '2026-11-15',
			to: '2026-11-21',
		});
		expect(periodRange('30d', NOW)).toEqual({
			from: '2026-10-23',
			to: '2026-11-21',
		});
		expect(periodRange('90d', NOW)).toEqual({
			from: '2026-08-24',
			to: '2026-11-21',
		});
	});

	it('formats local date parts, not UTC', () => {
		expect(localDate(new Date(2026, 0, 5, 23, 59))).toBe('2026-01-05');
	});
});

describe('dailyQuery', () => {
	it('drops empty filters and the unassigned marker', () => {
		expect(
			dailyQuery(
				{ from: 'a', to: 'b' },
				{ machineId: '', agent: 'codex', model: '', taskId: '' },
			),
		).toEqual({ from: 'a', to: 'b', agent: 'codex' });
		expect(
			dailyQuery(
				{ from: 'a', to: 'b' },
				{ machineId: 'm1', agent: '', model: 'x', taskId: '42' },
			),
		).toEqual({ from: 'a', to: 'b', machineId: 'm1', model: 'x', taskId: 42 });
		expect(
			dailyQuery(
				{ from: 'a', to: 'b' },
				{ machineId: '', agent: '', model: '', taskId: 'none' },
			),
		).toEqual({ from: 'a', to: 'b' });
	});
});

describe('filterOptions', () => {
	it('derives sorted unique options', () => {
		const opts = filterOptions(
			[
				row({ task_id: 7, task_key: 'TM-7' }),
				row({ task_id: 9, task_key: null, model: 'sonnet', agent: 'codex' }),
				row({ task_id: null }),
				row({ task_id: 7, task_key: 'TM-7' }),
			],
			[machine('m1', 'work'), machine('m2', 'home')],
		);
		expect(opts.machines).toEqual([
			{ id: 'm2', label: 'home' },
			{ id: 'm1', label: 'work' },
		]);
		expect(opts.agents).toEqual(['claude-code', 'codex']);
		expect(opts.models).toEqual(['opus', 'sonnet']);
		expect(opts.tasks).toEqual([
			{ id: '7', label: 'TM-7' },
			{ id: '9', label: '#9' },
		]);
		expect(opts.hasUnassigned).toBe(true);
	});

	it('treats task_id 0 as unassigned', () => {
		const opts = filterOptions([row({ task_id: 0 })], []);
		expect(opts.tasks).toEqual([]);
		expect(opts.hasUnassigned).toBe(true);
	});
});

describe('aggregateTotals', () => {
	it('sums tokens, cost, sessions and events', () => {
		expect(rowTokens(row())).toBe(15);
		expect(aggregateTotals([row(), row({ cost_usd: 0.5, events: 1 })])).toEqual(
			{ tokens: 30, cost: 2, sessions: 4, events: 11 },
		);
		expect(aggregateTotals([])).toEqual({
			tokens: 0,
			cost: 0,
			sessions: 0,
			events: 0,
		});
	});

	it('coerces decimal strings from the API', () => {
		expect(aggregateTotals([row({ cost_usd: '0.250000' as any })]).cost).toBe(
			0.25,
		);
	});
});

describe('buildDailySeries', () => {
	const machines = [machine('m1', 'work'), machine('m2', 'home')];

	it('fills missing days with zero and stacks by machine', () => {
		const s = buildDailySeries(
			[
				row({ day: '2026-11-19', machine_id: 'm1', cost_usd: 1 }),
				row({ day: '2026-11-19', machine_id: 'm2', cost_usd: 2 }),
				row({ day: '2026-11-21', machine_id: 'm1', cost_usd: 4 }),
			],
			{ from: '2026-11-19', to: '2026-11-21' },
			{ by: 'machine', metric: 'cost', machines },
		);
		expect(s.days.map((d) => d.day)).toEqual([
			'2026-11-19',
			'2026-11-20',
			'2026-11-21',
		]);
		expect(s.days.map((d) => d.total)).toEqual([3, 0, 4]);
		expect(s.days[1].parts).toEqual({});
		expect(s.series.map((x) => x.label)).toEqual(['work', 'home']);
		expect(s.days[0].parts).toEqual({ m1: 1, m2: 2 });
		expect(s.max).toBe(4);
	});

	it('stacks tokens by agent with display names', () => {
		const s = buildDailySeries(
			[
				row({ agent: 'codex' }),
				row({ agent: 'claude-code', input_tokens: 100 }),
			],
			{ from: '2026-11-20', to: '2026-11-20' },
			{ by: 'agent', metric: 'tokens', machines },
		);
		expect(s.series.map((x) => x.label)).toEqual(['Claude Code', 'Codex']);
		expect(s.days[0].total).toBe(15 + 114);
		expect(s.empty).toBe(false);
	});

	it('flags empty data and ignores rows outside the range', () => {
		const s = buildDailySeries(
			[row({ day: '2026-01-01' })],
			{ from: '2026-11-20', to: '2026-11-21' },
			{ by: 'agent', metric: 'cost', machines },
		);
		expect(s.empty).toBe(true);
		expect(s.days).toHaveLength(2);
	});

	it('falls back to the machine id when the machine is unknown', () => {
		const s = buildDailySeries(
			[row({ machine_id: 'gone' })],
			{ from: '2026-11-20', to: '2026-11-20' },
			{ by: 'machine', metric: 'cost', machines: [] },
		);
		expect(s.series[0].label).toBe('gone');
	});
});

describe('limitState', () => {
	const lim = (used_pct: number, resets_at: string | null) => ({
		agent: 'claude-code',
		account: 'default',
		window: 'weekly',
		used_pct,
		resets_at,
		plan: null,
		observed_at: '2026-11-21T11:00:00Z',
		source: 'statusline',
	});

	it('levels at 80 and 95', () => {
		expect(limitState(lim(79.9, null), NOW).level).toBe('ok');
		expect(limitState(lim(80, null), NOW).level).toBe('warn');
		expect(limitState(lim(94.9, null), NOW).level).toBe('warn');
		expect(limitState(lim(95, null), NOW).level).toBe('critical');
	});

	it('shows reset rows as 0 percent', () => {
		const s = limitState(lim(97, '2026-11-21T11:59:00Z'), NOW);
		expect(s).toEqual({ pct: 0, level: 'ok', reset: true });
	});

	it('clamps and keeps future resets', () => {
		expect(limitState(lim(130, '2026-11-22T00:00:00Z'), NOW)).toEqual({
			pct: 100,
			level: 'critical',
			reset: false,
		});
		expect(limitState(lim(-3, null), NOW).pct).toBe(0);
	});
});

describe('countdownText', () => {
	const at = (ms: number) => new Date(NOW.getTime() + ms).toISOString();
	const MIN = 60_000;
	it('formats days, hours, minutes', () => {
		expect(countdownText(at(52 * 60 * MIN), NOW)).toBe('2 d 4 h');
		expect(countdownText(at(192 * MIN), NOW)).toBe('3 h 12 m');
		expect(countdownText(at(12 * MIN + 5000), NOW)).toBe('12 m');
	});
	it('says resets soon under a minute or after the time', () => {
		expect(countdownText(at(30_000), NOW)).toBe('resets soon');
		expect(countdownText(at(-MIN), NOW)).toBe('resets soon');
	});
	it('is empty without a reset time', () => {
		expect(countdownText(null, NOW)).toBe('');
		expect(countdownText('garbage', NOW)).toBe('');
	});
});

describe('ageText', () => {
	const ago = (ms: number) => new Date(NOW.getTime() - ms).toISOString();
	it('formats age', () => {
		expect(ageText(ago(20_000), NOW)).toBe('just now');
		expect(ageText(ago(5 * 60_000), NOW)).toBe('5 m ago');
		expect(ageText(ago(3 * 3600_000 + 60_000), NOW)).toBe('3 h ago');
		expect(ageText(ago(2 * 86400_000), NOW)).toBe('2 d ago');
		expect(ageText('garbage', NOW)).toBe('');
	});
});

describe('names', () => {
	it('maps agents', () => {
		expect(agentName('claude-code')).toBe('Claude Code');
		expect(agentName('codex')).toBe('Codex');
		expect(agentName('cursor')).toBe('cursor');
	});
	it('maps windows', () => {
		expect(windowLabel('session')).toBe('5-hour');
		expect(windowLabel('weekly')).toBe('Weekly');
		expect(windowLabel('model:opus')).toBe('opus weekly');
	});
});

describe('limitRows', () => {
	const lim = (over: Record<string, unknown>) => ({
		agent: 'claude-code',
		account: 'default',
		window: 'weekly',
		used_pct: 50,
		resets_at: '2026-11-23T16:00:00Z',
		plan: null,
		observed_at: '2026-11-21T11:55:00Z',
		source: 'statusline',
		...over,
	});

	it('orders session before weekly and builds labels', () => {
		const rows = limitRows(
			[
				lim({ window: 'model:opus' }),
				lim({ window: 'weekly' }),
				lim({ window: 'session', resets_at: '2026-11-21T15:12:00Z' }),
			],
			NOW,
		);
		expect(rows.map((r) => r.label)).toEqual([
			'Claude Code · 5-hour',
			'Claude Code · Weekly',
			'Claude Code · opus weekly',
		]);
		expect(rows[0].countdown).toBe('3 h 12 m');
		expect(rows[1].countdown).toBe('2 d 4 h');
		expect(rows[0].asOf).toBe('updated 5 m ago · status line');
	});

	it('shows reset rows at zero without a countdown', () => {
		const [row] = limitRows(
			[lim({ used_pct: 99, resets_at: '2026-11-21T11:00:00Z' })],
			NOW,
		);
		expect(row).toMatchObject({
			pct: 0,
			reset: true,
			level: 'ok',
			countdown: '',
		});
	});

	it('carries the level and falls back to the raw source', () => {
		const [row] = limitRows([lim({ used_pct: 96, source: 'x-new' })], NOW);
		expect(row.level).toBe('critical');
		expect(row.asOf).toContain('x-new');
	});
});

describe('groupByModelTask', () => {
	it('merges rows per agent, model and task and sorts by cost', () => {
		const groups = groupByModelTask([
			row({ cost_usd: 1, task_id: 7, task_key: 'TM-7' }),
			row({ cost_usd: 2, task_id: 7, task_key: 'TM-7', day: '2026-11-19' }),
			row({ cost_usd: 5, task_id: null }),
			row({ cost_usd: 0.5, task_id: 0, model: 'sonnet' }),
		]);
		expect(groups.map((g) => [g.model, g.task, g.cost, g.sessions])).toEqual([
			['opus', null, 5, 2],
			['opus', 'TM-7', 3, 4],
			['sonnet', null, 0.5, 2],
		]);
	});
});
