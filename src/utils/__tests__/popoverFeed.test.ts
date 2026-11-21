import type { Machine } from '@/actions/tmgr/aiUsageMachines';
import type { RunnerSession } from '@/runner/api';
import type { RoutineEntry } from '@/types/dailyRoutine';
import type { TimeDailyDay } from '@/types/dashboard';
import type { UsageLimit, UsageOverview } from '@/usage/api';
import {
	buildMachines,
	buildTodayExtras,
	buildTodaySecondsByTask,
	buildUsageExtras,
	buildWorkspace,
} from '../popoverFeedModel';

const NOW = Date.parse('2026-11-21T12:00:00Z');
const NOW_S = NOW / 1000;

const limit = (over: Partial<UsageLimit> = {}): UsageLimit => ({
	tool: 'claude',
	account: 'a@example.com',
	window: 'five_hour',
	window_label: '5h',
	window_s: 18000,
	used_pct: 40,
	resets_at: null,
	resets_in_s: 600,
	plan: null,
	source: 'statusline',
	observed_at: '2026-11-21T11:59:00Z',
	age_s: 60,
	...over,
});

const overview = (over: Partial<UsageOverview> = {}): UsageOverview => ({
	supported: true,
	unsupported_reason: null,
	has_data: true,
	history_floor: null,
	accounts: [],
	windows: [],
	days: [],
	limits: [],
	...over,
});

const todayWindow = (cost: number, tokens: number) =>
	({
		key: 'today',
		label: 'Today',
		from: '2026-11-21',
		to: '2026-11-21',
		totals: {
			cost_usd: cost,
			tokens_total: tokens,
			input: 0,
			output: 0,
			cache_read: 0,
			cache_write: 0,
			reasoning: 0,
			events: 0,
			unpriced_events: 0,
			cost_by_type: { input: 0, output: 0, cache_read: 0, cache_write: 0 },
		},
		rows: [],
		projects: [],
	}) as UsageOverview['windows'][number];

const row = (tool: string, cost: number, tokens: number) =>
	({
		tool,
		model: 'm',
		cost_usd: cost,
		tokens_total: tokens,
	}) as UsageOverview['windows'][number]['rows'][number];

const withRows = (
	window: UsageOverview['windows'][number],
	rows: UsageOverview['windows'][number]['rows'],
) => ({ ...window, rows });

describe('buildUsageExtras', () => {
	it('picks the 5h and weekly limits per tool by exact window length', () => {
		const result = buildUsageExtras(
			overview({
				limits: [
					limit({ window_s: 18000, used_pct: 42 }),
					limit({ window_s: 604800, used_pct: 13, window: 'seven_day' }),
					limit({ window_s: 3600, used_pct: 99 }),
					limit({ tool: 'codex', window_s: 18000, used_pct: 7 }),
					limit({ tool: 'codex', window_s: 604800, used_pct: 55 }),
				],
			}),
			undefined,
			NOW,
		);
		expect(result?.providers).toEqual([
			{
				tool: 'claude',
				fiveHour: { pct: 42, resetsAt: NOW_S + 600, ageS: 60 },
				week: { pct: 13, resetsAt: NOW_S + 600, ageS: 60 },
			},
			{
				tool: 'codex',
				fiveHour: { pct: 7, resetsAt: NOW_S + 600, ageS: 60 },
				week: { pct: 55, resetsAt: NOW_S + 600, ageS: 60 },
			},
		]);
	});

	it('lists only providers that have data, claude before codex', () => {
		const result = buildUsageExtras(
			overview({ limits: [limit({ tool: 'codex' })] }),
			undefined,
			NOW,
		);
		expect(result?.providers?.map((p) => p.tool)).toEqual(['codex']);
	});

	it('ignores unknown tools', () => {
		const result = buildUsageExtras(
			overview({ limits: [limit({ tool: 'other', used_pct: 90 })] }),
			undefined,
			NOW,
		);
		expect(result).toBeUndefined();
	});

	it('takes the plan from the newest limit that has one', () => {
		const result = buildUsageExtras(
			overview({
				limits: [
					limit({ tool: 'codex', plan: 'pro' }),
					limit({ tool: 'codex', window_s: 604800, plan: null }),
				],
			}),
			undefined,
			NOW,
		);
		expect(result?.providers?.[0].plan).toBe('pro');
	});

	it('prefers the most recently observed limit of the same window', () => {
		const result = buildUsageExtras(
			overview({
				limits: [
					limit({ used_pct: 10, observed_at: '2026-11-21T10:00:00Z' }),
					limit({ used_pct: 70, observed_at: '2026-11-21T11:30:00Z' }),
				],
			}),
			undefined,
			NOW,
		);
		expect(result?.providers?.[0].fiveHour?.pct).toBe(70);
	});

	it('takes resets_at as absolute epoch seconds when present', () => {
		const result = buildUsageExtras(
			overview({
				limits: [limit({ resets_at: '2026-11-21T14:00:00Z', resets_in_s: 5 })],
			}),
			undefined,
			NOW,
		);
		expect(result?.providers?.[0].fiveHour?.resetsAt).toBe(
			Date.parse('2026-11-21T14:00:00Z') / 1000,
		);
	});

	it('leaves resetsAt null when neither reset source exists', () => {
		const result = buildUsageExtras(
			overview({ limits: [limit({ resets_at: null, resets_in_s: null })] }),
			undefined,
			NOW,
		);
		expect(result?.providers?.[0].fiveHour).toEqual({
			pct: 40,
			resetsAt: null,
			ageS: 60,
		});
	});

	it('omits ageS when the limit has no finite age', () => {
		const result = buildUsageExtras(
			overview({ limits: [limit({ age_s: Number.NaN })] }),
			undefined,
			NOW,
		);
		expect(result?.providers?.[0].fiveHour).not.toHaveProperty('ageS');
	});

	it('clamps the percentage into 0..100', () => {
		const result = buildUsageExtras(
			overview({ limits: [limit({ used_pct: 135 })] }),
			undefined,
			NOW,
		);
		expect(result?.providers?.[0].fiveHour?.pct).toBe(100);
	});

	it('reads today and week totals from their own windows', () => {
		const result = buildUsageExtras(
			overview({
				windows: [
					{ ...todayWindow(1.25, 5000), key: 'week' },
					todayWindow(0.5, 1200),
				],
			}),
			undefined,
			NOW,
		);
		expect(result).toEqual({
			todayCost: 0.5,
			todayTokens: 1200,
			weekCost: 1.25,
		});
	});

	it('splits today cost and tokens per tool when rows exist', () => {
		const result = buildUsageExtras(
			overview({
				limits: [limit(), limit({ tool: 'codex' })],
				windows: [
					withRows(todayWindow(5, 900), [
						row('claude', 1, 100),
						row('claude', 2, 300),
						row('codex', 2, 500),
					]),
				],
			}),
			undefined,
			NOW,
		);
		expect(result?.providers?.[0]).toMatchObject({
			tool: 'claude',
			todayCost: 3,
			todayTokens: 400,
		});
		expect(result?.providers?.[1]).toMatchObject({
			tool: 'codex',
			todayCost: 2,
			todayTokens: 500,
		});
	});

	it('adds a provider that has only today rows', () => {
		const result = buildUsageExtras(
			overview({
				windows: [withRows(todayWindow(2, 500), [row('codex', 2, 500)])],
			}),
			undefined,
			NOW,
		);
		expect(result?.providers).toEqual([
			{ tool: 'codex', todayCost: 2, todayTokens: 500 },
		]);
	});

	it('keeps a zero today cost as a real value', () => {
		const result = buildUsageExtras(
			overview({ windows: [todayWindow(0, 0)] }),
			undefined,
			NOW,
		);
		expect(result).toEqual({ todayCost: 0, todayTokens: 0 });
	});

	it('omits every field when there is no source', () => {
		expect(buildUsageExtras(overview(), undefined, NOW)).toBeUndefined();
		expect(buildUsageExtras(null, undefined, NOW)).toBeUndefined();
	});

	it('passes a finite running cost through', () => {
		expect(buildUsageExtras(null, 0.37, NOW)).toEqual({ runningCost: 0.37 });
		expect(buildUsageExtras(null, Number.NaN, NOW)).toBeUndefined();
	});
});

describe('buildTodayExtras', () => {
	const routine = (over: Partial<RoutineEntry> = {}): RoutineEntry =>
		({
			task_id: 7,
			title: 'Standup',
			date: '2026-11-21',
			completed: false,
			...over,
		}) as RoutineEntry;

	it('hides the whole block when tracked time is unknown', () => {
		expect(
			buildTodayExtras({
				trackedSeconds: null,
				routines: [routine()],
				today: '2026-11-21',
			}),
		).toBeUndefined();
	});

	it('keeps a zero tracked time as a real value', () => {
		expect(
			buildTodayExtras({
				trackedSeconds: 0,
				routines: [],
				today: '2026-11-21',
			}),
		).toEqual({ trackedSeconds: 0, routines: [], deadlines: [] });
	});

	it('maps routines due today to id, title and done', () => {
		const result = buildTodayExtras({
			trackedSeconds: 120,
			routines: [
				routine({ task_id: 1, title: 'A', completed: true }),
				routine({ task_id: 2, title: 'B', completed: false }),
			],
			today: '2026-11-21',
		});
		expect(result?.routines).toEqual([
			{ id: 1, title: 'A', done: true },
			{ id: 2, title: 'B', done: false },
		]);
	});

	it('drops routines dated on other days and dedupes by task', () => {
		const result = buildTodayExtras({
			trackedSeconds: 0,
			routines: [
				routine({ task_id: 1, date: '2026-11-20' }),
				routine({ task_id: 2, completed: false }),
				routine({ task_id: 2, completed: true }),
			],
			today: '2026-11-21',
		});
		expect(result?.routines).toEqual([
			{ id: 2, title: 'Standup', done: false },
		]);
	});

	it('has no deadline source, so deadlines stay empty', () => {
		expect(
			buildTodayExtras({
				trackedSeconds: 5,
				routines: null,
				today: '2026-11-21',
			})?.deadlines,
		).toEqual([]);
	});
});

describe('buildMachines', () => {
	const session = (kind: RunnerSession['state']['kind']): RunnerSession =>
		({
			id: kind,
			name: kind,
			profile: null,
			state: { kind, sinceMs: 0, detail: null },
			createdMs: 0,
		}) as RunnerSession;

	const cloud = (over: Partial<Machine> = {}): Machine => ({
		machine_id: 'cloud-1',
		label: 'Build box',
		platform: 'linux',
		cli_version: null,
		timezone: null,
		last_seen_at: new Date(NOW - 60_000).toISOString(),
		limits: [],
		...over,
	});

	it('reports the local runner as busy when any session is working', () => {
		const result = buildMachines(
			[session('working'), session('waiting')],
			null,
			NOW,
			'This computer',
		);
		expect(result).toEqual([
			{
				id: 'local',
				label: 'This computer',
				state: 'busy',
				sessions: 2,
				local: true,
			},
		]);
	});

	it('reports the local runner as online with no working session', () => {
		const result = buildMachines([session('waiting')], null, NOW, 'Mine');
		expect(result?.[0]).toMatchObject({ state: 'online', sessions: 1 });
	});

	it('does not count exited sessions', () => {
		const result = buildMachines(
			[session('exited'), session('exited'), session('working')],
			null,
			NOW,
			'Mine',
		);
		expect(result?.[0]).toMatchObject({ state: 'busy', sessions: 1 });
	});

	it('shows a local runner with zero sessions as online', () => {
		const result = buildMachines([], null, NOW, 'Mine');
		expect(result).toEqual([
			{ id: 'local', label: 'Mine', state: 'online', sessions: 0, local: true },
		]);
	});

	it('marks cloud machines online within ten minutes and offline after', () => {
		const result = buildMachines(
			null,
			[
				cloud({
					machine_id: 'fresh',
					last_seen_at: new Date(NOW - 600_000).toISOString(),
				}),
				cloud({
					machine_id: 'stale',
					last_seen_at: new Date(NOW - 600_001).toISOString(),
				}),
			],
			NOW,
			'Mine',
		);
		expect(result).toEqual([
			{
				id: 'fresh',
				label: 'Build box',
				state: 'online',
				sessions: null,
				local: false,
			},
			{
				id: 'stale',
				label: 'Build box',
				state: 'offline',
				sessions: null,
				local: false,
			},
		]);
	});

	it('marks a cloud machine with an unreadable timestamp offline', () => {
		const result = buildMachines(
			null,
			[cloud({ last_seen_at: 'nope' })],
			NOW,
			'Mine',
		);
		expect(result?.[0]).toMatchObject({ state: 'offline', sessions: null });
	});

	it('dedupes cloud machines by id and skips ones without id', () => {
		const result = buildMachines(
			null,
			[cloud(), cloud(), cloud({ machine_id: '' })],
			NOW,
			'Mine',
		);
		expect(result).toHaveLength(1);
	});

	it('omits the block when both sources are unavailable', () => {
		expect(buildMachines(null, null, NOW, 'Mine')).toBeUndefined();
	});

	it('keeps local and cloud entries together, local first', () => {
		const result = buildMachines([session('working')], [cloud()], NOW, 'Mine');
		expect(result?.map((m) => m.id)).toEqual(['local', 'cloud-1']);
		expect(result?.[1].local).toBe(false);
	});
});

describe('buildTodaySecondsByTask', () => {
	const day = (date: string, items: [number, number][]): TimeDailyDay => ({
		date,
		total_s: 0,
		items: items.map(([id, seconds]) => ({
			task: { id, title: `T${id}` },
			category: null,
			seconds,
		})),
	});

	it('keys totals by workspace and task for recent tasks only', () => {
		const result = buildTodaySecondsByTask(
			56,
			[1, 2],
			[
				day('2026-11-21', [
					[1, 600],
					[2, 300],
					[3, 900],
				]),
			],
			'2026-11-21',
		);
		expect(result).toEqual({ '56:1': 600, '56:2': 300 });
	});

	it('sums repeated items for the same task', () => {
		const result = buildTodaySecondsByTask(
			56,
			[1],
			[
				day('2026-11-21', [
					[1, 100],
					[1, 50],
				]),
			],
			'2026-11-21',
		);
		expect(result).toEqual({ '56:1': 150 });
	});

	it('ignores days other than today', () => {
		const result = buildTodaySecondsByTask(
			56,
			[1],
			[day('2026-11-20', [[1, 999]])],
			'2026-11-21',
		);
		expect(result).toBeUndefined();
	});

	it('omits the map when nothing matches', () => {
		expect(buildTodaySecondsByTask(56, [], [], '2026-11-21')).toBeUndefined();
	});
});

describe('buildWorkspace', () => {
	it('returns name and code when both are present', () => {
		expect(buildWorkspace({ name: 'Work', code: 'work' })).toEqual({
			name: 'Work',
			code: 'work',
		});
	});

	it('omits the workspace when the store has none', () => {
		expect(buildWorkspace(null)).toBeUndefined();
		expect(buildWorkspace({ name: 'Work' })).toBeUndefined();
	});
});
