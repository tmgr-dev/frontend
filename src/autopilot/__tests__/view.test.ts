import type { AutopilotInput, StatusDoc } from '../types';
import {
	buildView,
	cleanText,
	clock,
	money,
	safeHttpsUrl,
	span,
	toolName,
	windowName,
} from '../view';

const NOW = Date.parse('2026-10-19T20:03:00Z');
const TZ = 120;

const doc = (over: Partial<StatusDoc> = {}): StatusDoc => ({
	version: 1,
	state: 'running',
	pid: 4242,
	control_seq: 0,
	settings_id: 'ap-20261019-175000',
	started_at: '2026-10-19T17:50:00Z',
	updated_at: '2026-10-19T20:03:00Z',
	rule: 'tm',
	mode: 'auto',
	settings: {
		usage_guard: true,
		feedback_status: 'Needs feedback',
		blocked_status: 'Blocked',
	},
	tasks: {
		total: 100,
		done_pr: 23,
		in_work: 4,
		needs_feedback: 9,
		blocked: 2,
		held: 1,
		failed: 3,
		skipped: 0,
		queued: 58,
	},
	teams: { running: 2, max: 2 },
	agents: { running: 5, max: 6 },
	runs: [
		{
			task: 'TM-12',
			task_id: 12,
			title: 'Fix it',
			mode: 'auto',
			started_at: '2026-10-19T19:49:00Z',
			repos: ['o/api', 'o/web'],
			step: 'running',
		},
	],
	limits: [
		{
			tool: 'claude',
			window: 'five_hour',
			used_pct: 41,
			resets_at: '2026-10-19T19:10:00Z',
			source: 'real',
			origin: 'get_usage',
			observed_at: '2026-10-19T20:01:00Z',
			stop_new_at: 50,
			pause_at: 80,
		},
		{ tool: 'claude', window: 'seven_day', used_pct: null, source: 'unknown' },
	],
	cost: {
		subscription_usd: 0,
		subscription_tokens: 0,
		api_usd: 3.4,
		api_cap_usd: 20,
		billing: 'api',
		basis: '',
	},
	next_stop: { window: 'five_hour', pct_left: 9, kind: 'no new tasks' },
	unconfirmed_new: 0,
	failures_in_row: 0,
	prs: [],
	events: [],
	...over,
});

const input = (over: Partial<AutopilotInput> = {}): AutopilotInput => ({
	running: true,
	pid: 4242,
	status: doc(),
	...over,
});

const at = (minutesFromStart: number): string =>
	new Date(
		Date.UTC(2026, 9, 19, 17, 0) + minutesFromStart * 60000,
	).toISOString();

describe('span', () => {
	it.each([
		[0, '0s'],
		[5, '5s'],
		[59, '59s'],
		[60, '1m'],
		[125, '2m'],
		[8040, '2h 14m'],
		[90000, '25h 0m'],
		[-30, '0s'],
	])('span(%p) is %p', (secs, want) => {
		expect(span(secs)).toBe(want);
	});

	it('returns a dash for non-finite input', () => {
		expect(span(Number.NaN)).toBe('—');
	});
});

describe('clock', () => {
	it('formats the instant in the given offset', () => {
		expect(clock('2026-10-19T17:50:00Z', 120)).toBe('19:50');
		expect(clock('2026-10-19T17:50:00Z', -300)).toBe('12:50');
	});

	it('wraps across midnight', () => {
		expect(clock('2026-10-19T23:30:00Z', 120)).toBe('01:30');
	});

	it('uses the local zone when no offset is given', () => {
		const t = Date.parse('2026-10-19T17:50:00Z');
		const local = new Date(t);
		const pad = (n: number) => String(n).padStart(2, '0');
		expect(clock('2026-10-19T17:50:00Z')).toBe(
			`${pad(local.getHours())}:${pad(local.getMinutes())}`,
		);
	});

	it('returns a placeholder for an invalid time', () => {
		expect(clock('nope', 0)).toBe('--:--');
	});
});

describe('money', () => {
	it.each([
		[0, '$0'],
		[3.4, '$3.40'],
		[20, '$20'],
		[12.3, '$12.30'],
		[0.004, '$0'],
		[null, 'n/a'],
		[undefined, 'n/a'],
		[Number.NaN, 'n/a'],
	])('money(%p) is %p', (v, want) => {
		expect(money(v)).toBe(want);
	});
});

describe('names', () => {
	it('capitalises the tool name like the CLI', () => {
		expect(toolName('claude')).toBe('Claude');
		expect(toolName('')).toBe('');
	});

	it('maps window ids to short names', () => {
		expect(windowName('five_hour')).toBe('5h');
		expect(windowName('seven_day')).toBe('week');
		expect(windowName('monthly')).toBe('monthly');
	});
});

describe('cleanText', () => {
	it('strips escape sequences and other control characters', () => {
		expect(cleanText('x\u001b[31mred\u0007', 100)).toBe('x[31mred');
	});

	it('strips bidi override and isolate characters', () => {
		expect(cleanText('a‮b⁦c⁩d', 100)).toBe('abcd');
	});

	it('turns line breaks into spaces', () => {
		expect(cleanText('one\ntwo\r\tthree', 100)).toBe('one two  three');
	});

	it('clips to the given length with an ellipsis', () => {
		expect(cleanText('abcdef', 4)).toBe('abc…');
		expect(cleanText('abcd', 4)).toBe('abcd');
	});
});

describe('safeHttpsUrl', () => {
	it('keeps a normalised https url', () => {
		expect(safeHttpsUrl('https://github.com/o/api/pull/1')).toBe(
			'https://github.com/o/api/pull/1',
		);
	});

	it.each([
		['http://example.com/pr/1'],
		['javascript:alert(1)'],
		['https://user:secret@example.com/pr/1'],
		['https://user@example.com/pr/1'],
		['not a url'],
		[`https://example.com/${'a'.repeat(2050)}`],
		[42],
		[null],
	])('rejects %p', (url) => {
		expect(safeHttpsUrl(url)).toBeNull();
	});
});

describe('buildView header', () => {
	it('shows starting while running before the first status doc', () => {
		const view = buildView({ running: true, pid: 1, status: null }, NOW, {
			tzMinutes: TZ,
		});
		expect(view.empty).toBe(false);
		expect(view.header).toMatchObject({
			title: 'Autopilot',
			word: 'starting',
			startedAt: null,
			runningFor: null,
		});
		expect(view.counters).toEqual([]);
	});

	it('reports a running autopilot with clock, span and slots', () => {
		const view = buildView(input(), NOW, { tzMinutes: TZ });
		expect(view.header).toEqual({
			title: 'Autopilot',
			word: 'running',
			note: null,
			needsUpdate: false,
			startedAt: '19:50',
			runningFor: '2h 13m',
			rule: 'tm',
			mode: 'auto',
			stopReason: null,
			stale: false,
			agents: '5/6',
			teams: '2/2',
		});
	});

	it('names a paused autopilot', () => {
		const view = buildView(input({ status: doc({ state: 'paused' }) }), NOW, {
			tzMinutes: TZ,
		});
		expect(view.header?.word).toBe('paused');
	});

	it('names a stopping autopilot', () => {
		const view = buildView(input({ status: doc({ state: 'stopping' }) }), NOW, {
			tzMinutes: TZ,
		});
		expect(view.header?.word).toBe('stopping');
	});

	it('names a suspended autopilot while it runs', () => {
		const view = buildView(
			input({ status: doc({ state: 'suspended' }) }),
			NOW,
			{ tzMinutes: TZ },
		);
		expect(view.header?.word).toBe('suspended');
	});

	it('reports not running with the last state and its age', () => {
		const view = buildView(
			{
				running: false,
				pid: null,
				status: doc({ state: 'paused', updated_at: '2026-10-19T20:00:00Z' }),
			},
			NOW,
			{ tzMinutes: TZ },
		);
		expect(view.empty).toBe(false);
		expect(view.header).toMatchObject({
			word: 'not running',
			note: 'last state paused, updated 3m ago',
		});
	});

	it('reports zero seconds for an update at the same instant', () => {
		const view = buildView(
			{ running: false, pid: null, status: doc({ state: 'paused' }) },
			NOW,
			{ tzMinutes: TZ },
		);
		expect(view.header?.note).toBe('last state paused, updated 0s ago');
	});

	it('reports stopped with the stop reason and no note', () => {
		const view = buildView(
			{
				running: false,
				pid: null,
				status: doc({
					state: 'stopped',
					stop_reason: 'usage\u0007 limit\nnow',
				}),
			},
			NOW,
			{ tzMinutes: TZ },
		);
		expect(view.header).toMatchObject({
			word: 'stopped',
			note: null,
			stopReason: 'usage limit now',
		});
	});

	it('shows the stopped word while running even with a stopped doc', () => {
		const view = buildView(input({ status: doc({ state: 'stopped' }) }), NOW, {
			tzMinutes: TZ,
		});
		expect(view.header?.word).toBe('stopped');
	});

	it('uses the queue loop title for a queue doc', () => {
		const view = buildView(input({ status: doc({ kind: 'queue' }) }), NOW, {
			tzMinutes: TZ,
		});
		expect(view.header?.title).toBe('Queue loop');
	});

	it('flags an unknown state and shows it raw', () => {
		const view = buildView(input({ status: doc({ state: 'weird' }) }), NOW, {
			tzMinutes: TZ,
		});
		expect(view.header).toMatchObject({
			word: 'weird',
			needsUpdate: true,
			note: null,
		});
		expect(view.buttons).toEqual({ pause: false, resume: false, stop: false });
	});

	it('flags a status doc of another version and shows its state raw', () => {
		const view = buildView(input({ status: doc({ version: 2 }) }), NOW, {
			tzMinutes: TZ,
		});
		expect(view.header).toMatchObject({ word: 'running', needsUpdate: true });
	});

	it('marks a running doc stale when it has not updated for over 30s', () => {
		const stale = buildView(
			input({ status: doc({ updated_at: '2026-10-19T20:02:00Z' }) }),
			NOW,
			{ tzMinutes: TZ },
		);
		const fresh = buildView(
			input({ status: doc({ updated_at: '2026-10-19T20:02:45Z' }) }),
			NOW,
			{ tzMinutes: TZ },
		);
		expect(stale.header?.stale).toBe(true);
		expect(fresh.header?.stale).toBe(false);
	});

	it('is not stale when not running', () => {
		const view = buildView(
			{
				running: false,
				pid: null,
				status: doc({ updated_at: '2026-10-19T19:00:00Z' }),
			},
			NOW,
			{ tzMinutes: TZ },
		);
		expect(view.header?.stale).toBe(false);
	});
});

describe('buildView empty state', () => {
	it('is empty when not running and there is no status doc', () => {
		const view = buildView({ running: false, pid: null, status: null }, NOW);
		expect(view).toMatchObject({
			empty: true,
			emptyText: 'Autopilot is not running.',
			emptyCommand: 'tmgr queue autopilot',
			header: null,
			counters: [],
			runs: [],
			limits: [],
			cost: null,
			events: [],
		});
		expect(view.buttons).toEqual({ pause: false, resume: false, stop: false });
	});
});

describe('buildView buttons', () => {
	it.each([
		['running', true, { pause: true, resume: false, stop: true }],
		['paused', true, { pause: false, resume: true, stop: true }],
		['stopping', true, { pause: false, resume: false, stop: false }],
		['suspended', true, { pause: false, resume: false, stop: false }],
		['running', false, { pause: false, resume: false, stop: false }],
	])('state %s running=%p gives %p', (state, running, want) => {
		const view = buildView(
			{ running, pid: null, status: doc({ state }) },
			NOW,
			{ tzMinutes: TZ },
		);
		expect(view.buttons).toEqual(want);
	});
});

describe('buildView counters', () => {
	it('lists the seven counters in order with their counts', () => {
		const view = buildView(input(), NOW, { tzMinutes: TZ });
		expect(view.counters.map((c) => [c.key, c.label, c.count])).toEqual([
			['done', 'done (PR)', 23],
			['inWork', 'in work', 4],
			['needsFeedback', 'needs feedback', 9],
			['blocked', 'blocked', 2],
			['held', 'held', 1],
			['failed', 'failed', 3],
			['queued', 'queued', 58],
		]);
	});

	it('takes in-work items from the active runs', () => {
		const view = buildView(input(), NOW, { tzMinutes: TZ });
		expect(view.counters[1].items).toEqual([
			{ task: 'TM-12', title: 'Fix it' },
		]);
	});

	it('takes done items from PRs and keeps only safe urls', () => {
		const view = buildView(
			input({
				status: doc({
					prs: [
						{
							task: 'TM-1',
							url: 'https://github.com/o/api/pull/1',
							draft: false,
						},
						{ task: 'TM-2', url: 'javascript:alert(1)', draft: true },
					],
				}),
			}),
			NOW,
			{ tzMinutes: TZ },
		);
		expect(view.counters[0].items).toEqual([
			{ task: 'TM-1', url: 'https://github.com/o/api/pull/1' },
			{ task: 'TM-2' },
		]);
	});

	it('takes held and failed items from resume, then from summary', () => {
		const fromResume = buildView(
			input({
				status: doc({
					resume: { held: [{ task: 'TM-7', reason: 'no spec' }] },
					summary: { failed: [{ task: 'TM-8', reason: 'old' }] },
				}),
			}),
			NOW,
			{ tzMinutes: TZ },
		);
		expect(fromResume.counters[4].items).toEqual([
			{ task: 'TM-7', reason: 'no spec' },
		]);
		expect(fromResume.counters[5].items).toEqual([
			{ task: 'TM-8', reason: 'old' },
		]);

		const fromSummary = buildView(
			input({
				status: doc({
					summary: { failed: [{ task: 'TM-9', reason: 'boom' }] },
				}),
			}),
			NOW,
			{ tzMinutes: TZ },
		);
		expect(fromSummary.counters[5].items).toEqual([
			{ task: 'TM-9', reason: 'boom' },
		]);
	});

	it('leaves held and failed items null without a source', () => {
		const view = buildView(input(), NOW, { tzMinutes: TZ });
		expect(view.counters[4].items).toBeNull();
		expect(view.counters[5].items).toBeNull();
	});

	it('marks needs feedback, blocked and queued as not clickable', () => {
		const view = buildView(input(), NOW, { tzMinutes: TZ });
		expect(view.counters[2].items).toBeNull();
		expect(view.counters[3].items).toBeNull();
		expect(view.counters[6].items).toBeNull();
	});
});

describe('buildView runs', () => {
	it('computes duration and joins nothing, keeping repos as a list', () => {
		const view = buildView(input(), NOW, { tzMinutes: TZ });
		expect(view.runs).toEqual([
			{
				task: 'TM-12',
				taskId: 12,
				title: 'Fix it',
				mode: 'auto',
				duration: '14m',
				repos: ['o/api', 'o/web'],
				step: 'running',
			},
		]);
	});

	it('shows a dash when the run has no start time', () => {
		const view = buildView(
			input({ status: doc({ runs: [{ task: 'TM-3', started_at: null }] }) }),
			NOW,
			{ tzMinutes: TZ },
		);
		expect(view.runs[0].duration).toBe('-');
	});

	it('attaches the PR url of its task only when it is safe', () => {
		const view = buildView(
			input({
				status: doc({
					runs: [
						{ task: 'TM-1', started_at: null },
						{ task: 'TM-2', started_at: null },
					],
					prs: [
						{ task: 'TM-1', url: 'https://github.com/o/api/pull/1' },
						{ task: 'TM-2', url: 'javascript:alert(1)' },
					],
				}),
			}),
			NOW,
			{ tzMinutes: TZ },
		);
		expect(view.runs[0].prUrl).toBe('https://github.com/o/api/pull/1');
		expect(view.runs[1]).not.toHaveProperty('prUrl');
	});

	it('keeps optional run fields when present', () => {
		const view = buildView(
			input({
				status: doc({
					runs: [
						{
							task: 'TM-4',
							started_at: null,
							workspace_code: 'tmgrdev',
							session_id: 'abc',
							agents: 3,
						},
					],
				}),
			}),
			NOW,
			{ tzMinutes: TZ },
		);
		expect(view.runs[0]).toMatchObject({
			workspaceCode: 'tmgrdev',
			sessionId: 'abc',
			agents: '3',
		});
	});

	it('cleans control characters from server text', () => {
		const view = buildView(
			input({
				status: doc({
					runs: [{ task: 'TM-5', started_at: null, step: 'x\u001b[31mred' }],
				}),
			}),
			NOW,
			{ tzMinutes: TZ },
		);
		expect(view.runs[0].step).toBe('x[31mred');
	});
});

describe('buildView limits', () => {
	it('renders the known limit exactly as the CLI does', () => {
		const view = buildView(input(), NOW, { tzMinutes: TZ });
		expect(view.limits[0]).toEqual({
			label: 'Claude 5h',
			text: 'Claude 5h: 41% real 2m ago (stop new at 50%, pause at 80%) · resets 21:10',
			usedPct: 41,
			source: 'real',
			soft: 50,
			hard: 80,
			resets: '21:10',
			age: '2m',
		});
	});

	it('renders an unknown limit without a percentage', () => {
		const view = buildView(input(), NOW, { tzMinutes: TZ });
		expect(view.limits[1]).toEqual({
			label: 'Claude week',
			text: 'Claude week: unknown',
			usedPct: null,
			source: 'unknown',
			soft: null,
			hard: null,
			resets: null,
			age: null,
		});
	});

	it('omits the stop marks unless both thresholds are set', () => {
		const view = buildView(
			input({
				status: doc({
					limits: [
						{
							tool: 'claude',
							window: 'five_hour',
							used_pct: 12,
							source: 'estimate',
							stop_new_at: 50,
						},
					],
				}),
			}),
			NOW,
			{ tzMinutes: TZ },
		);
		expect(view.limits[0]).toMatchObject({
			text: 'Claude 5h: 12% estimate',
			soft: null,
			hard: null,
		});
	});

	it('explains the missing limits when the guard is off', () => {
		const off = buildView(
			input({ status: doc({ limits: [], settings: { usage_guard: false } }) }),
			NOW,
			{ tzMinutes: TZ },
		);
		expect(off.limitsNote).toBe('no usage limit (the guard is off)');
	});

	it('gives no limits note while the guard is on', () => {
		const on = buildView(input({ status: doc({ limits: [] }) }), NOW, {
			tzMinutes: TZ,
		});
		expect(on.limitsNote).toBeNull();
	});
});

describe('buildView next stop', () => {
	it('describes the share of the window left before the next stop', () => {
		const view = buildView(input(), NOW, { tzMinutes: TZ });
		expect(view.nextStop).toBe(
			'≈ 9% of the 5h window left before "no new tasks"',
		);
	});

	it('is null without a next stop', () => {
		const view = buildView(input({ status: doc({ next_stop: null }) }), NOW, {
			tzMinutes: TZ,
		});
		expect(view.nextStop).toBeNull();
	});
});

describe('buildView cost', () => {
	it.each([
		[
			{ subscription_usd: 0, api_usd: 3.4, api_cap_usd: 20 },
			'$0',
			null,
			'$3.40 of $20 cap',
		],
		[
			{ subscription_usd: 0, api_usd: null, api_cap_usd: 20 },
			'$0',
			null,
			'none yet, $20 cap',
		],
		[
			{ subscription_usd: 0, api_usd: 3.4, api_cap_usd: null },
			'$0',
			null,
			'$3.40',
		],
		[
			{ subscription_usd: 0, api_usd: null, api_cap_usd: null },
			'$0',
			null,
			'n/a',
		],
		[
			{
				subscription_usd: 12.3,
				basis: 'estimate (list price)',
				api_usd: null,
				api_cap_usd: null,
			},
			'$12.30',
			'estimate (list price)',
			'n/a',
		],
		[
			{ subscription_usd: 0, basis: 'estimate (list price)' },
			'$0',
			null,
			'n/a',
		],
	])('cost %j', (cost, subscription, basis, api) => {
		const view = buildView(input({ status: doc({ cost }) }), NOW, {
			tzMinutes: TZ,
		});
		expect(view.cost).toEqual({ subscription, basis, api, tokens: 0 });
	});
});

describe('buildView events', () => {
	it('lists events newest first and caps them at 50', () => {
		const events = Array.from({ length: 60 }, (_, i) => ({
			at: at(i),
			kind: 'picked',
			task: `TM-${i}`,
			text: 'started',
		}));
		const view = buildView(input({ status: doc({ events }) }), NOW, {
			tzMinutes: TZ,
		});
		expect(view.events).toHaveLength(50);
		expect(view.events[0].task).toBe('TM-59');
		expect(view.events[49].task).toBe('TM-10');
	});

	it('formats event time and falls back when it is missing', () => {
		const view = buildView(
			input({
				status: doc({
					events: [
						{
							at: '2026-10-19T17:52:00Z',
							kind: 'pr',
							task: 'TM-1',
							text: 'opened',
						},
						{ kind: 'note', text: 'no time' },
					],
				}),
			}),
			NOW,
			{ tzMinutes: TZ },
		);
		expect(view.events[0]).toEqual({
			at: '--:--',
			kind: 'note',
			task: null,
			text: 'no time',
		});
		expect(view.events[1]).toEqual({
			at: '19:52',
			kind: 'pr',
			task: 'TM-1',
			text: 'opened',
		});
	});

	it('cleans event text and keeps only safe urls', () => {
		const view = buildView(
			input({
				status: doc({
					events: [
						{ kind: 'pr', text: '‮evil\u0007', url: 'javascript:alert(1)' },
						{ kind: 'pr', text: 'ok', url: 'https://github.com/o/api/pull/2' },
					],
				}),
			}),
			NOW,
			{ tzMinutes: TZ },
		);
		expect(view.events[0].url).toBe('https://github.com/o/api/pull/2');
		expect(view.events[1]).not.toHaveProperty('url');
		expect(view.events[1].text).toBe('evil');
	});
});

describe('buildView misc', () => {
	it('passes through unconfirmed new tasks', () => {
		const view = buildView(
			input({ status: doc({ unconfirmed_new: 4 }) }),
			NOW,
			{ tzMinutes: TZ },
		);
		expect(view.unconfirmedNew).toBe(4);
	});

	it('survives a malformed status doc', () => {
		const view = buildView(
			{
				running: true,
				pid: 1,
				status: {
					version: 1,
					state: 'running',
					runs: 'nope',
					limits: [null, 7],
					events: 'x',
					tasks: 'y',
				} as unknown as StatusDoc,
			},
			NOW,
			{ tzMinutes: TZ },
		);
		expect(view.runs).toEqual([]);
		expect(view.events).toEqual([]);
		expect(view.limits).toHaveLength(2);
		expect(view.counters[0].count).toBe(0);
	});
});
