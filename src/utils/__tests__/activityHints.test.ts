import { invoke } from '@tauri-apps/api/core';
import {
	ACTIVITY_APPS,
	activityEnabledFor,
	buildTimeEntry,
	entryOutcome,
	formatSuggestionRange,
	loadActivityConsent,
	minutesOf,
	registerActivityContext,
	resetActivitySync,
	saveActivityConsent,
	stopActivity,
	syncActivity,
	type ActivityContext,
} from '../activityHints';

jest.mock('@tauri-apps/api/core', () => ({
	invoke: jest.fn(),
}));

const storage = new Map<string, string>();
const invokeMock = invoke as jest.Mock;

const base = {
	consent: true,
	supported: true,
	mode: 'ask' as const,
	folders: ['/repo'],
	modulesKnown: true,
	sessionTimeOn: true,
};

const context = (over: Partial<ActivityContext> = {}): ActivityContext => ({
	userId: 3,
	workspaceId: 5,
	timerRunning: false,
	modulesKnown: true,
	sessionTimeOn: true,
	...over,
});

const updates = () =>
	invokeMock.mock.calls
		.filter(([name]) => name === 'activity_update')
		.map(([, args]) => args);

beforeEach(() => {
	storage.clear();
	Object.defineProperty(globalThis, 'localStorage', {
		configurable: true,
		value: {
			getItem: (key: string) => (storage.has(key) ? storage.get(key)! : null),
			setItem: (key: string, value: string) => storage.set(key, String(value)),
			removeItem: (key: string) => storage.delete(key),
		},
	});
	resetActivitySync();
	registerActivityContext(null);
	invokeMock.mockReset();
	invokeMock.mockImplementation((name: string) =>
		Promise.resolve(name === 'activity_supported' ? true : undefined),
	);
});

describe('activity consent', () => {
	it('defaults to false', () => {
		expect(loadActivityConsent()).toBe(false);
	});

	it('treats invalid stored values as false', () => {
		storage.set('app.activity.enabled', 'yes');
		expect(loadActivityConsent()).toBe(false);
	});

	it('round-trips true and false', () => {
		saveActivityConsent(true);
		expect(storage.get('app.activity.enabled')).toBe('true');
		expect(loadActivityConsent()).toBe(true);
		saveActivityConsent(false);
		expect(loadActivityConsent()).toBe(false);
	});
});

describe('activityEnabledFor', () => {
	it('is on when every condition holds', () => {
		expect(activityEnabledFor(base)).toBe(true);
	});

	it.each([
		['no consent', { consent: false }],
		['unsupported platform', { supported: false }],
		['git tracking off', { mode: 'off' as const }],
		['no folders', { folders: [] }],
		['modules not known yet', { modulesKnown: false }],
		['session_time off or missing', { sessionTimeOn: false }],
	])('is off with %s', (_name, over) => {
		expect(activityEnabledFor({ ...base, ...over })).toBe(false);
	});
});

describe('syncActivity', () => {
	it('does nothing on unsupported platforms', async () => {
		invokeMock.mockImplementation((name: string) =>
			Promise.resolve(name === 'activity_supported' ? false : undefined),
		);
		saveActivityConsent(true);
		registerActivityContext(() => context());
		await syncActivity();
		expect(updates()).toEqual([]);
	});

	it('pushes enabled with ids and the timer state', async () => {
		saveActivityConsent(true);
		localStorage.setItem('app.gitTrack.folders', JSON.stringify(['/repo']));
		registerActivityContext(() => context({ timerRunning: true }));
		await syncActivity();
		expect(updates()).toEqual([
			{ enabled: true, userId: 3, workspaceId: 5, timerRunning: true },
		]);
	});

	it('pushes disabled without consent', async () => {
		localStorage.setItem('app.gitTrack.folders', JSON.stringify(['/repo']));
		registerActivityContext(() => context());
		await syncActivity();
		expect(updates()[0]).toMatchObject({ enabled: false });
	});

	it.each([
		['unknown modules', { modulesKnown: false }],
		['session_time off', { sessionTimeOn: false }],
	])('pushes disabled with %s', async (_name, over) => {
		saveActivityConsent(true);
		localStorage.setItem('app.gitTrack.folders', JSON.stringify(['/repo']));
		registerActivityContext(() => context(over));
		await syncActivity();
		expect(updates()[0]).toMatchObject({ enabled: false });
	});

	it('pushes disabled with no context or user', async () => {
		saveActivityConsent(true);
		await syncActivity();
		registerActivityContext(() => context({ userId: null }));
		await syncActivity();
		expect(updates().every((u) => u.enabled === false)).toBe(true);
	});

	it('skips an identical repeat and pushes on change', async () => {
		saveActivityConsent(true);
		localStorage.setItem('app.gitTrack.folders', JSON.stringify(['/repo']));
		let running = false;
		registerActivityContext(() => context({ timerRunning: running }));
		await syncActivity();
		await syncActivity();
		running = true;
		await syncActivity();
		expect(updates().map((u) => u.timerRunning)).toEqual([false, true]);
	});

	it('stopActivity pushes disabled and lets the next sync push again', async () => {
		saveActivityConsent(true);
		localStorage.setItem('app.gitTrack.folders', JSON.stringify(['/repo']));
		registerActivityContext(() => context());
		await syncActivity();
		await stopActivity();
		await syncActivity();
		expect(updates().map((u) => u.enabled)).toEqual([true, false, true]);
	});
});

describe('buildTimeEntry', () => {
	const suggestion = {
		id: 'act-0123456789abcdef',
		key: 'TM-1',
		startedAt: Date.UTC(2026, 9, 21, 10, 0, 0) / 1000,
		endedAt: Date.UTC(2026, 9, 21, 10, 20, 0) / 1000,
		seconds: 900,
	};

	it('maps a suggestion to an activity time entry', () => {
		expect(buildTimeEntry(suggestion)).toEqual({
			client_interval_id: 'act-0123456789abcdef',
			source: 'activity',
			started_at: '2026-10-21T10:00:00.000Z',
			ended_at: '2026-10-21T10:20:00.000Z',
			seconds: 900,
		});
	});

	it('caps seconds at the span and floors to an integer', () => {
		expect(buildTimeEntry({ ...suggestion, seconds: 5000 }).seconds).toBe(1200);
		expect(buildTimeEntry({ ...suggestion, seconds: 899.9 }).seconds).toBe(899);
	});
});

describe('entryOutcome', () => {
	it.each([
		['created', 'done'],
		['duplicate', 'done'],
		['conflict', 'conflict'],
		['weird', 'error'],
		[undefined, 'error'],
	])('%s -> %s', (status, outcome) => {
		expect(entryOutcome(status as string | undefined)).toBe(outcome);
	});
});

describe('format helpers', () => {
	it('rounds to whole minutes, at least one', () => {
		expect(minutesOf(900)).toBe(15);
		expect(minutesOf(95)).toBe(2);
		expect(minutesOf(10)).toBe(1);
	});

	it('formats a same-day range', () => {
		const start = new Date(2026, 9, 21, 10, 0).getTime() / 1000;
		const end = new Date(2026, 9, 21, 10, 20).getTime() / 1000;
		expect(formatSuggestionRange(start, end)).toEqual({
			date: '21 Oct 2026',
			range: '10:00–10:20',
		});
	});
});

describe('ACTIVITY_APPS', () => {
	it('lists display names only', () => {
		expect(ACTIVITY_APPS.length).toBeGreaterThan(3);
		expect(new Set(ACTIVITY_APPS).size).toBe(ACTIVITY_APPS.length);
	});
});
