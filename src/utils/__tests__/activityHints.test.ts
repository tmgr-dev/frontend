import { invoke } from '@tauri-apps/api/core';
import {
	activityEnabledFor,
	buildTimeEntry,
	createAnnouncer,
	entryOutcome,
	formatSuggestionRange,
	keyMatchesWorkspace,
	listActivityApps,
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
		expect(loadActivityConsent(3)).toBe(false);
	});

	it('treats invalid stored values as false', () => {
		storage.set('app.activity.enabled.3', 'yes');
		expect(loadActivityConsent(3)).toBe(false);
	});

	it('round-trips true and false per user', () => {
		saveActivityConsent(3, true);
		expect(storage.get('app.activity.enabled.3')).toBe('true');
		expect(loadActivityConsent(3)).toBe(true);
		expect(loadActivityConsent(4)).toBe(false);
		saveActivityConsent(3, false);
		expect(loadActivityConsent(3)).toBe(false);
	});

	it('ignores the old shared key and missing users', () => {
		storage.set('app.activity.enabled', 'true');
		expect(loadActivityConsent(3)).toBe(false);
		expect(loadActivityConsent(null)).toBe(false);
		saveActivityConsent(null, true);
		expect(storage.size).toBe(1);
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
	const withFolders = () =>
		localStorage.setItem('app.gitTrack.folders', JSON.stringify(['/repo']));

	it('does nothing on unsupported platforms', async () => {
		invokeMock.mockImplementation((name: string) =>
			Promise.resolve(name === 'activity_supported' ? false : undefined),
		);
		saveActivityConsent(3, true);
		registerActivityContext(() => context());
		await syncActivity();
		expect(updates()).toEqual([]);
	});

	it('pushes consent and collecting with ids and the timer state', async () => {
		saveActivityConsent(3, true);
		withFolders();
		registerActivityContext(() => context({ timerRunning: true }));
		await syncActivity();
		expect(updates()).toEqual([
			{
				consent: true,
				collecting: true,
				userId: 3,
				workspaceId: 5,
				timerRunning: true,
			},
		]);
	});

	it('pushes consent false without consent', async () => {
		withFolders();
		registerActivityContext(() => context());
		await syncActivity();
		expect(updates()[0]).toMatchObject({ consent: false, collecting: false });
	});

	it('does not read another user consent', async () => {
		saveActivityConsent(9, true);
		withFolders();
		registerActivityContext(() => context());
		await syncActivity();
		expect(updates()[0]).toMatchObject({ consent: false, collecting: false });
	});

	it.each([
		['unknown modules', { modulesKnown: false }],
		['session_time off', { sessionTimeOn: false }],
	])('keeps consent but pauses with %s', async (_name, over) => {
		saveActivityConsent(3, true);
		withFolders();
		registerActivityContext(() => context(over));
		await syncActivity();
		expect(updates()[0]).toMatchObject({ consent: true, collecting: false });
	});

	it('keeps consent but pauses when git tracking is off', async () => {
		saveActivityConsent(3, true);
		withFolders();
		localStorage.setItem('app.gitTrack.mode', 'off');
		registerActivityContext(() => context());
		await syncActivity();
		expect(updates()[0]).toMatchObject({ consent: true, collecting: false });
	});

	it('pauses without revoking consent with no context or user', async () => {
		saveActivityConsent(3, true);
		await syncActivity();
		registerActivityContext(() => context({ userId: null }));
		await syncActivity();
		expect(
			updates().every((u) => u.consent === true && u.collecting === false),
		).toBe(true);
	});

	it('skips an identical repeat and pushes on change', async () => {
		saveActivityConsent(3, true);
		withFolders();
		let running = false;
		registerActivityContext(() => context({ timerRunning: running }));
		await syncActivity();
		await syncActivity();
		running = true;
		await syncActivity();
		expect(updates().map((u) => u.timerRunning)).toEqual([false, true]);
	});

	it('stopActivity pauses without revoking consent and lets the next sync push again', async () => {
		saveActivityConsent(3, true);
		withFolders();
		registerActivityContext(() => context());
		await syncActivity();
		await stopActivity();
		await syncActivity();
		expect(updates().map((u) => [u.consent, u.collecting])).toEqual([
			[true, true],
			[true, false],
			[true, true],
		]);
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

describe('listActivityApps', () => {
	it('returns the display names from the app', async () => {
		invokeMock.mockResolvedValue(['Cursor', 'Xcode']);
		expect(await listActivityApps()).toEqual(['Cursor', 'Xcode']);
		expect(invokeMock).toHaveBeenCalledWith('activity_apps', undefined);
	});

	it('is empty when the command fails', async () => {
		invokeMock.mockRejectedValue(new Error('no'));
		expect(await listActivityApps()).toEqual([]);
	});
});

describe('keyMatchesWorkspace', () => {
	it('matches the prefix of the key against the workspace prefixes', () => {
		expect(keyMatchesWorkspace('TM-157', ['TM', 'WEB'])).toBe(true);
		expect(keyMatchesWorkspace('tm-157', ['TM'])).toBe(true);
		expect(keyMatchesWorkspace('MOBILE-APP-24', ['MOBILE-APP'])).toBe(true);
		expect(keyMatchesWorkspace('API-3', ['TM'])).toBe(false);
		expect(keyMatchesWorkspace('TM-1', [])).toBe(false);
	});
});

describe('createAnnouncer', () => {
	const at = (id: string, startedAt: number) => ({
		id,
		key: 'TM-1',
		startedAt,
		endedAt: startedAt + 900,
		seconds: 900,
	});
	const now = Math.floor(Date.now() / 1000);
	const old = now - 3 * 86400;

	it('announces each suggestion once', () => {
		const announce = createAnnouncer();
		expect(announce('a', [at('1', now)]).map((s) => s.id)).toEqual(['1']);
		expect(announce('a', [at('1', now)])).toEqual([]);
		expect(
			announce('a', [at('1', now), at('2', now)]).map((s) => s.id),
		).toEqual(['2']);
	});

	it('skips older days on the first poll of a scope', () => {
		const announce = createAnnouncer();
		expect(
			announce('a', [at('1', old), at('2', now)]).map((s) => s.id),
		).toEqual(['2']);
		expect(announce('a', [at('1', old)])).toEqual([]);
	});

	it('starts over when the scope changes', () => {
		const announce = createAnnouncer();
		announce('a', [at('1', now)]);
		expect(announce('b', [at('1', now)]).map((s) => s.id)).toEqual(['1']);
	});
});
