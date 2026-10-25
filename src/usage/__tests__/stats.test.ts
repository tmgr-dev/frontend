import type { UsageDay, UsageProjectRow, UsageRow } from '../api';
import {
	averageCostPerDay,
	buildDailyChart,
	dayKey,
	lastDayKeys,
	niceTicks,
	OTHER_KEY,
	projectLines,
	tokenTypeRows,
	weekComparison,
} from '../stats';

const TODAY = new Date(2026, 9, 25, 15, 0);

const ago = (n: number) => dayKey(new Date(2026, 9, 25 - n));

const day = (
	n: number,
	tool: string,
	model: string,
	cost: number,
	tokens = 100,
	events = 1,
): UsageDay => ({
	day: ago(n),
	tool,
	model,
	cost_usd: cost,
	tokens_total: tokens,
	events,
});

const row = (over: Partial<UsageRow> = {}): UsageRow => ({
	cost_usd: 0,
	tokens_total: 0,
	input: 0,
	output: 0,
	cache_read: 0,
	cache_write: 0,
	reasoning: 0,
	events: 0,
	unpriced_events: 0,
	cost_by_type: { input: 0, output: 0, cache_read: 0, cache_write: 0 },
	...over,
});

describe('day keys', () => {
	it('lists the last N local days, oldest first, across a month boundary', () => {
		const keys = lastDayKeys(new Date(2026, 10, 2, 23, 59), 4);
		expect(keys).toEqual([
			'2026-10-30',
			'2026-10-31',
			'2026-11-01',
			'2026-11-02',
		]);
	});
});

describe('niceTicks', () => {
	it('picks 1/2/5 steps that cover the max', () => {
		expect(niceTicks(12.34)).toEqual([0, 5, 10, 15]);
		expect(niceTicks(100)).toEqual([0, 50, 100]);
		expect(niceTicks(7)).toEqual([0, 2, 4, 6, 8]);
		expect(niceTicks(1200)).toEqual([0, 500, 1000, 1500]);
	});

	it('keeps cents steps for tiny and zero maxima', () => {
		expect(niceTicks(0.003)).toEqual([0, 0.01]);
		expect(niceTicks(0.3)).toEqual([0, 0.1, 0.2, 0.3]);
		expect(niceTicks(0)).toEqual([0, 1]);
	});
});

describe('buildDailyChart', () => {
	it('fills every one of the 30 days, gaps with zero', () => {
		const chart = buildDailyChart(
			[day(0, 'claude', 'opus', 5), day(10, 'claude', 'opus', 2)],
			'model',
			TODAY,
		);
		expect(chart.days).toHaveLength(30);
		expect(chart.days[29].day).toBe(ago(0));
		expect(chart.days[0].day).toBe(ago(29));
		expect(chart.days[28]).toMatchObject({
			cost: 0,
			tokens: 0,
			events: 0,
			parts: [],
		});
		expect(chart.days[19].cost).toBe(2);
		expect(chart.empty).toBe(false);
	});

	it('sums cost, tokens and events per day across tools and models', () => {
		const chart = buildDailyChart(
			[
				day(0, 'claude', 'opus', 3, 1000, 4),
				day(0, 'codex', 'gpt', 1.5, 500, 2),
				day(0, 'claude', 'haiku', 0.5, 50, 1),
			],
			'tool',
			TODAY,
		);
		expect(chart.days[29]).toMatchObject({ cost: 5, tokens: 1550, events: 7 });
		expect(chart.series.map((s) => s.key)).toEqual(['claude', 'codex']);
		expect(chart.days[29].parts).toEqual([
			{ key: 'claude', cost: 3.5 },
			{ key: 'codex', cost: 1.5 },
		]);
		expect(chart.series[0].label).toBe('Claude Code');
	});

	it('keeps the top 5 models and groups the rest as Other', () => {
		const models = ['m1', 'm2', 'm3', 'm4', 'm5', 'm6', 'm7'];
		const days = models.map((m, i) => day(0, 'claude', m, 10 - i));
		const chart = buildDailyChart(days, 'model', TODAY);
		expect(chart.series.map((s) => s.key)).toEqual([
			'm1',
			'm2',
			'm3',
			'm4',
			'm5',
			OTHER_KEY,
		]);
		expect(chart.series[5]).toMatchObject({ label: 'Other', other: true });
		expect(chart.days[29].parts[5]).toEqual({
			key: OTHER_KEY,
			cost: 5 + 4,
		});
		expect(chart.days[29].cost).toBe(10 + 9 + 8 + 7 + 6 + 5 + 4);
	});

	it('ranks models over the whole window, and adds no Other for five or fewer', () => {
		const chart = buildDailyChart(
			[
				day(0, 'claude', 'small', 1),
				day(5, 'claude', 'big', 9),
				day(6, 'claude', '', 0.5),
			],
			'model',
			TODAY,
		);
		expect(chart.series.map((s) => s.label)).toEqual([
			'big',
			'small',
			'Unknown model',
		]);
	});

	it('ignores entries outside the 30-day window', () => {
		const chart = buildDailyChart(
			[day(30, 'claude', 'opus', 99), day(-1, 'claude', 'opus', 99)],
			'model',
			TODAY,
		);
		expect(chart.empty).toBe(true);
		expect(chart.series).toEqual([]);
	});

	it('is empty without usage and keeps a drawable axis', () => {
		const chart = buildDailyChart([], 'tool', TODAY);
		expect(chart.empty).toBe(true);
		expect(chart.days).toHaveLength(30);
		expect(chart.axisMax).toBe(1);
	});

	it('sets the axis from the tallest day', () => {
		const chart = buildDailyChart(
			[day(1, 'claude', 'opus', 12.34), day(2, 'claude', 'opus', 3)],
			'tool',
			TODAY,
		);
		expect(chart.ticks).toEqual([0, 5, 10, 15]);
		expect(chart.axisMax).toBe(15);
	});

	it('does not call a day with tokens but no price empty', () => {
		const chart = buildDailyChart(
			[day(0, 'claude', 'opus', 0, 500, 2)],
			'tool',
			TODAY,
		);
		expect(chart.empty).toBe(false);
		expect(chart.days[29]).toMatchObject({ cost: 0, tokens: 500, parts: [] });
	});
});

describe('weekComparison', () => {
	it('compares the last 7 local days with the 7 before', () => {
		const days = [
			day(0, 'claude', 'a', 6, 600, 6),
			day(6, 'claude', 'a', 4, 400, 4),
			day(7, 'claude', 'a', 5, 250, 2),
			day(13, 'claude', 'a', 5, 250, 3),
			day(14, 'claude', 'a', 100, 9999, 99),
		];
		const w = weekComparison(days, TODAY);
		expect(w.cost).toEqual({ current: 10, previous: 10, pct: 0 });
		expect(w.tokens).toEqual({ current: 1000, previous: 500, pct: 100 });
		expect(w.events).toEqual({ current: 10, previous: 5, pct: 100 });
	});

	it('reports a decrease as a negative change', () => {
		const w = weekComparison(
			[day(0, 'claude', 'a', 3), day(8, 'claude', 'a', 4)],
			TODAY,
		);
		expect(w.cost.pct).toBeCloseTo(-25);
	});

	it('has no percentage when the previous 7 days are empty', () => {
		const w = weekComparison([day(2, 'claude', 'a', 3)], TODAY);
		expect(w.cost).toEqual({ current: 3, previous: 0, pct: null });
		expect(w.events.pct).toBeNull();
	});
});

describe('averageCostPerDay', () => {
	it('divides by 30 for a full history', () => {
		const avg = averageCostPerDay(
			[day(29, 'claude', 'a', 15), day(0, 'claude', 'a', 15)],
			TODAY,
		);
		expect(avg).toEqual({ perDay: 1, spanDays: 30 });
	});

	it('divides by the days since the first usage for a new user', () => {
		const avg = averageCostPerDay(
			[day(3, 'claude', 'a', 6), day(0, 'codex', 'b', 2)],
			TODAY,
		);
		expect(avg).toEqual({ perDay: 2, spanDays: 4 });
	});

	it('counts at least one day, also when the first usage is today', () => {
		expect(averageCostPerDay([day(0, 'claude', 'a', 3)], TODAY)).toEqual({
			perDay: 3,
			spanDays: 1,
		});
	});

	it('is zero without usage', () => {
		expect(averageCostPerDay([], TODAY)).toEqual({ perDay: 0, spanDays: 30 });
	});
});

const project = (
	key: string,
	name: string,
	cost: number,
	tokens = 10,
): UsageProjectRow => ({
	...row({ cost_usd: cost, tokens_total: tokens, events: 2 }),
	key,
	name,
});

describe('projectLines', () => {
	it('labels by folder name and sorts by cost then tokens', () => {
		const { lines, more } = projectLines([
			project('aaa111aaa111', 'web', 1, 5),
			project('bbb222bbb222', 'api', 3),
			project('ccc333ccc333', 'docs', 1, 50),
		]);
		expect(lines.map((l) => l.label)).toEqual(['api', 'docs', 'web']);
		expect(more).toBe(0);
	});

	it('adds a short key to duplicate names only', () => {
		const { lines } = projectLines([
			project('abcdef123456', 'app', 5),
			project('fedcba654321', 'app', 4),
			project('111111111111', 'lib', 3),
		]);
		expect(lines.map((l) => l.label)).toEqual([
			'app · abcdef',
			'app · fedcba',
			'lib',
		]);
	});

	it('shows No project for an empty name and keeps duplicates apart', () => {
		const { lines } = projectLines([
			project('', '', 5),
			project('123456abcdef', '  ', 4),
		]);
		expect(lines.map((l) => l.label)).toEqual([
			'No project',
			'No project · 123456',
		]);
	});

	it('shows the top 10 and counts the rest', () => {
		const all = Array.from({ length: 13 }, (_, i) =>
			project(`k${i}`.padEnd(12, '0'), `p${i}`, 100 - i),
		);
		const { lines, more } = projectLines(all);
		expect(lines).toHaveLength(10);
		expect(lines[9].label).toBe('p9');
		expect(more).toBe(3);
	});

	it('disambiguates by the full list, not only the visible top 10', () => {
		const all = [
			...Array.from({ length: 10 }, (_, i) =>
				project(`k${i}`.padEnd(12, '0'), `p${i}`, 100 - i),
			),
			project('zzzzzz999999', 'p0', 1),
		];
		const { lines } = projectLines(all);
		expect(lines[0].label).toBe('p0 · k00000');
	});
});

describe('tokenTypeRows', () => {
	it('lists the four types with shares and costs', () => {
		const { rows, noSplit } = tokenTypeRows(
			row({
				cost_usd: 10,
				input: 100,
				output: 100,
				cache_read: 700,
				cache_write: 100,
				tokens_total: 1000,
				cost_by_type: { input: 2, output: 5, cache_read: 1, cache_write: 2 },
			}),
		);
		expect(rows.map((r) => [r.label, r.tokens, r.share, r.cost])).toEqual([
			['Input', 100, 10, 2],
			['Output', 100, 10, 5],
			['Cache read', 700, 70, 1],
			['Cache write', 100, 10, 2],
		]);
		expect(noSplit).toBeNull();
	});

	it('shows the unsplit remainder as No price split', () => {
		const { noSplit } = tokenTypeRows(
			row({
				cost_usd: 10,
				input: 10,
				cost_by_type: { input: 6, output: 0, cache_read: 0, cache_write: 0 },
			}),
		);
		expect(noSplit).toBeCloseTo(4);
	});

	it('shows the whole cost as unsplit when the model had no rates', () => {
		expect(tokenTypeRows(row({ cost_usd: 3, input: 10 })).noSplit).toBe(3);
	});

	it('ignores a remainder of half a cent or less', () => {
		const { noSplit } = tokenTypeRows(
			row({
				cost_usd: 10.004,
				cost_by_type: { input: 10, output: 0, cache_read: 0, cache_write: 0 },
			}),
		);
		expect(noSplit).toBeNull();
	});

	it('has zero shares for an empty period', () => {
		const { rows, noSplit } = tokenTypeRows(row());
		expect(rows.every((r) => r.share === 0)).toBe(true);
		expect(noSplit).toBeNull();
	});
});
