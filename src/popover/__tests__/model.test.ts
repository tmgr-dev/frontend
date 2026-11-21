import type { CardView } from '@/decisions/types';
import {
	actionIds,
	ageText,
	durationText,
	formatClock,
	hasUsage,
	isAnswerable,
	isStale,
	markBusy,
	moneyText,
	nextIndex,
	nextResize,
	orderOptions,
	panelMaxHeight,
	pendingDecisions,
	resetText,
	runningElapsed,
	stripTicket,
	ticketCode,
	todayEntries,
	tokensText,
	usageLevel,
	visibleSections,
} from '../model';
import type { PopoverModel } from '../types';

const card = (overrides: Partial<CardView> = {}): CardView =>
	({
		id: 'c1',
		created_ms: 0,
		state: 'open',
		session_gone: false,
		can_reply: true,
		listening: false,
		unpicked: false,
		messages: [],
		source: { agent: 'orchestrator' },
		kind: 'decision',
		blocking: false,
		title: 'Tag now?',
		facts: [],
		options: [
			{ id: 'hold', label: 'Hold', recommended: false },
			{ id: 'release', label: 'Release', recommended: true },
		],
		diffs: [],
		patches: [],
		checks: [],
		links: {},
		reply_via: 'mcp',
		...overrides,
	}) as CardView;

const model = (overrides: Partial<PopoverModel> = {}): PopoverModel => ({
	platform: 'macos',
	version: '0.9.169',
	now: 1_000_000,
	running: [],
	recent: [],
	pluginSections: [],
	dnd: { option: 'off', until: null },
	autostart: false,
	updaterEnabled: false,
	decisionsAvailable: false,
	openDecisions: 0,
	extras: {},
	missed: [],
	maxHeight: 760,
	...overrides,
});

describe('usageLevel', () => {
	it.each([
		[0, 'ok'],
		[70, 'ok'],
		[71, 'warn'],
		[94, 'warn'],
		[95, 'crit'],
		[100, 'crit'],
	] as const)('maps %i%% to %s', (pct, level) => {
		expect(usageLevel(pct)).toBe(level);
	});
});

describe('resetText', () => {
	const now = 1_000_000;

	it('returns null without a reset time', () => {
		expect(resetText(null, now)).toBeNull();
	});

	it('formats hours and minutes under a day', () => {
		expect(resetText(now + 4 * 3600 + 5 * 60, now)).toBe('4h 05m');
	});

	it('formats minutes under an hour', () => {
		expect(resetText(now + 18 * 60, now)).toBe('18m');
	});

	it('shows a weekday beyond 24 hours', () => {
		expect(resetText(now + 3 * 86400, now)).toMatch(/^[A-Z][a-z]{2}$/);
	});
});

describe('durations and clocks', () => {
	it('formats the live clock as H:MM:SS', () => {
		expect(formatClock(5047)).toBe('1:24:07');
		expect(formatClock(-3)).toBe('0:00:00');
	});

	it('formats today time as Xh MMm or Xm', () => {
		expect(durationText(3900)).toBe('1h 05m');
		expect(durationText(35 * 60)).toBe('35m');
		expect(durationText(0)).toBe('0m');
	});

	it('formats tokens and money', () => {
		expect(tokensText(4_200_000)).toBe('4.2M');
		expect(tokensText(12_000)).toBe('12K');
		expect(tokensText(950)).toBe('950');
		expect(moneyText(18.4)).toBe('$18.40');
	});

	it('formats decision age', () => {
		expect(ageText(0, 3 * 60_000)).toBe('3m');
		expect(ageText(0, 2 * 3600_000)).toBe('2h');
		expect(ageText(0, 3 * 86400_000)).toBe('3d');
	});

	it('adds the live offset to committed time for a running task', () => {
		const task = {
			id: 1,
			workspaceId: 1,
			title: 'x',
			commonTime: 60,
			startTime: 100,
		};
		expect(runningElapsed(task, 130)).toBe(90);
		expect(runningElapsed({ ...task, startTime: 0 }, 130)).toBe(60);
	});
});

describe('ticketCode', () => {
	it('extracts the code before the first colon-space', () => {
		expect(ticketCode('TM-1019: E2E relay: runner')).toBe('TM-1019');
		expect(stripTicket('TM-1019: E2E relay: runner')).toBe('E2E relay: runner');
	});

	it('accepts uppercase and digit prefixes', () => {
		expect(ticketCode('AB2-7: x')).toBe('AB2-7');
	});

	it.each([
		'Plain title',
		'tm-1019: lower case',
		'TM-: no number',
		'TM-10a: letters',
		'A-B-1: nested dash',
		'TM-1019:no space',
		'-1: no prefix',
	])('returns null for %s', (title) => {
		expect(ticketCode(title)).toBeNull();
		expect(stripTicket(title)).toBe(title);
	});
});

describe('nextResize', () => {
	it('skips an unchanged height and re-sends after a reset to zero', () => {
		expect(nextResize(300.2, 600, 0)).toBe(301);
		expect(nextResize(300.2, 600, 301)).toBeNull();
		expect(nextResize(300.2, 600, 0)).toBe(301);
	});

	it('caps at the maximum', () => {
		expect(nextResize(900, 600, 0)).toBe(600);
		expect(nextResize(900, 600, 600)).toBeNull();
	});
});

describe('nextIndex', () => {
	it('moves down and wraps to the first row', () => {
		expect(nextIndex(0, 3, 1)).toBe(1);
		expect(nextIndex(2, 3, 1)).toBe(0);
	});

	it('moves up and wraps to the last row', () => {
		expect(nextIndex(1, 3, -1)).toBe(0);
		expect(nextIndex(0, 3, -1)).toBe(2);
	});

	it('starts from the edge when nothing is highlighted', () => {
		expect(nextIndex(-1, 3, 1)).toBe(0);
		expect(nextIndex(-1, 3, -1)).toBe(2);
	});

	it('returns -1 when there are no rows', () => {
		expect(nextIndex(-1, 0, 1)).toBe(-1);
	});
});

describe('action ids', () => {
	it('builds the same ids as the native tray menu', () => {
		expect(actionIds.open(5, 1019)).toBe('open:5:1019');
		expect(actionIds.stop(5, 1019)).toBe('stop:5:1019');
		expect(actionIds.switch(5, 1020)).toBe('switch:5:1020');
		expect(actionIds.dnd('tomorrow')).toBe('dnd:tomorrow');
		expect(actionIds.routineDone(9)).toBe('routine-done:9');
		expect(actionIds.pluginItem('pomodoro.pause')).toBe(
			'plugin-item:pomodoro.pause',
		);
	});
});

describe('decision ordering and answerability', () => {
	it('puts the recommended option first and keeps the rest in order', () => {
		const options = [
			{ id: 'a', label: 'A', recommended: false },
			{ id: 'b', label: 'B', recommended: true },
			{ id: 'c', label: 'C', recommended: false },
		];
		expect(orderOptions(options).map((o) => o.id)).toEqual(['b', 'a', 'c']);
	});

	it('keeps only pending cards', () => {
		const cards = [
			card({ id: 'open', state: 'open' }),
			card({ id: 'deferred', state: 'deferred' }),
			card({ id: 'done', state: 'answered' }),
		];
		expect(pendingDecisions(cards).map((c) => c.id)).toEqual([
			'open',
			'deferred',
		]);
	});

	it('is answerable only with options and no action', () => {
		expect(isAnswerable(card())).toBe(true);
		expect(isAnswerable(card({ options: [] }))).toBe(false);
		expect(isAnswerable(card({ state: 'answered' }))).toBe(false);
		expect(isAnswerable(card({ reply_via: 'hook' }))).toBe(false);
		expect(isAnswerable(card({ reply_via: 'hook', push: true }))).toBe(true);
	});
});

describe('section visibility', () => {
	it('shows nothing optional for an empty model', () => {
		expect(visibleSections(model())).toEqual([]);
	});

	it('shows every section when all data is present', () => {
		const full = model({
			decisionsAvailable: true,
			missed: [{ id: 'm', kind: 'agent', title: 't', body: 'b', at: 1 }],
			recent: [
				{
					id: 1,
					workspaceId: 1,
					title: 'TM-1: x',
					commonTime: 0,
					startTime: 0,
				},
			],
			pluginSections: [{ pluginName: 'p', label: 'P', items: [] }],
			extras: {
				today: { trackedSeconds: 0, routines: [], deadlines: [] },
				machines: [
					{ id: 'm', label: 'Mac', state: 'online', sessions: 1, local: true },
				],
				usage: { todayCost: 1 },
			},
		});
		expect(visibleSections(full)).toEqual([
			'decisions',
			'missed',
			'switch',
			'today',
			'machines',
			'ai',
			'plugins',
		]);
	});

	it('hides machines and today when their extras are absent or empty', () => {
		const sparse = model({ extras: { machines: [] } });
		expect(visibleSections(sparse)).toEqual([]);
	});

	it('treats usage as present only when some value is set', () => {
		expect(hasUsage(undefined)).toBe(false);
		expect(hasUsage({})).toBe(false);
		expect(hasUsage({ runningCost: 0 })).toBe(true);
		expect(hasUsage({ weekCost: 4 })).toBe(true);
		expect(hasUsage({ providers: [] })).toBe(false);
		expect(hasUsage({ providers: [{ tool: 'codex', plan: 'pro' }] })).toBe(
			true,
		);
	});
});

describe('todayEntries', () => {
	const routine = (id: number) => ({ id, title: `r${id}`, done: false });
	const deadline = (id: number, dueAt: number) => ({
		id,
		workspaceId: 1,
		title: `d${id}`,
		code: null,
		dueAt,
	});

	it('lists routines first, then the nearest deadlines, up to the cap', () => {
		const result = todayEntries(
			[routine(1), routine(2)],
			[deadline(3, 30), deadline(4, 10), deadline(5, 20), deadline(6, 40)],
		);
		expect(result.routines.map((r) => r.id)).toEqual([1, 2]);
		expect(result.deadlines.map((d) => d.id)).toEqual([4, 5, 3]);
		expect(result.hiddenRoutines).toBe(0);
	});

	it('reserves the last slot for the nearest deadline when routines fill the cap', () => {
		const routines = [1, 2, 3, 4, 5, 6, 7, 8].map(routine);
		const result = todayEntries(routines, [deadline(9, 5), deadline(10, 1)]);
		expect(result.routines).toHaveLength(4);
		expect(result.deadlines.map((d) => d.id)).toEqual([10]);
		expect(result.hiddenRoutines).toBe(4);
	});

	it('counts only hidden routines in the more label', () => {
		const result = todayEntries(
			[routine(1)],
			[1, 2, 3, 4, 5, 6].map((i) => deadline(i, i)),
		);
		expect(result.deadlines).toHaveLength(4);
		expect(result.hiddenRoutines).toBe(0);
	});

	it('hides nothing when everything fits', () => {
		const result = todayEntries([routine(1)], []);
		expect(result.hiddenRoutines).toBe(0);
	});

	it('does not mutate its input', () => {
		const deadlines = [deadline(1, 9), deadline(2, 1)];
		todayEntries([], deadlines);
		expect(deadlines.map((d) => d.id)).toEqual([1, 2]);
	});
});

describe('isStale', () => {
	it('flags a bar observed more than an hour ago', () => {
		expect(isStale({ pct: 1, resetsAt: null, ageS: 3601 })).toBe(true);
		expect(isStale({ pct: 1, resetsAt: null, ageS: 3600 })).toBe(false);
	});

	it('never flags a bar without an age', () => {
		expect(isStale({ pct: 1, resetsAt: null })).toBe(false);
	});
});

describe('panelMaxHeight', () => {
	it('leaves room for the transparent padding', () => {
		expect(panelMaxHeight(760)).toBe(736);
		expect(panelMaxHeight(10)).toBe(0);
	});
});

describe('markBusy', () => {
	it('adds an id once without mutating the input', () => {
		const busy = ['a'];
		expect(markBusy(busy, 'b')).toEqual(['a', 'b']);
		expect(markBusy(['a'], 'a')).toEqual(['a']);
		expect(busy).toEqual(['a']);
	});
});
