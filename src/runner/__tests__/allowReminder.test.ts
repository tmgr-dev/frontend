import type { AllowedEntry } from '@/utils/agentApprovalPolicy';

jest.mock('@tauri-apps/api/core', () => ({ invoke: jest.fn() }));
jest.mock('@/utils/desktop', () => ({ isDesktopApp: () => false }));

import { invoke } from '@tauri-apps/api/core';
import {
	allowCommand,
	checkAllowReminders,
	dueReminders,
	localDay,
} from '../allowReminder';

const entry = (code: string, expiresAt: string | null): AllowedEntry => ({
	workspaceCode: code,
	workspaceId: 1,
	expiresAt,
	expired: false,
	keys: 1,
	repos: 0,
});

describe('dueReminders', () => {
	const now = Date.parse('2026-10-19T12:00:00Z');
	const list = [
		entry('a', '2026-10-21T00:00:00Z'),
		entry('b', '2026-10-30T00:00:00Z'),
		entry('c', null),
	];

	it('returns expiring entries not yet shown today', () => {
		expect(dueReminders(list, now, {}).map((e) => e.workspaceCode)).toEqual([
			'a',
		]);
		expect(dueReminders(list, now, { a: localDay(now) })).toEqual([]);
		expect(
			dueReminders(list, now, { a: '2000-01-01' }).map((e) => e.workspaceCode),
		).toEqual(['a']);
	});

	it('builds the allow command', () => {
		expect(allowCommand('tmgrdev')).toBe('tmgr runner allow tmgrdev');
	});
});

describe('checkAllowReminders', () => {
	const now = Date.parse('2026-10-19T12:00:00Z');
	const day = 86_400_000;
	const mocked = invoke as jest.Mock;
	let storage: Record<string, string>;
	let entries: AllowedEntry[];

	const notifications = () =>
		mocked.mock.calls.filter(([command]) => command === 'plugin_notify');

	beforeEach(() => {
		storage = {};
		(globalThis as any).localStorage = {
			getItem: (key: string) => storage[key] ?? null,
			setItem: (key: string, value: string) => {
				storage[key] = value;
			},
		};
		mocked.mockReset();
		mocked.mockImplementation(async (command: string) =>
			command === 'runner_allowed_list' ? { entries, error: null } : undefined,
		);
		entries = [];
	});

	afterEach(() => {
		delete (globalThis as any).localStorage;
	});

	it('notifies once for a workspace expiring within three days, with the renew command', async () => {
		entries = [entry('alpha', new Date(now + 2 * day).toISOString())];
		await checkAllowReminders(now);
		expect(mocked).toHaveBeenCalledWith('runner_allowed_list');
		expect(notifications()).toEqual([
			[
				'plugin_notify',
				{
					title: 'Cloud agent runs expire soon',
					body: 'Workspace alpha stops accepting agent runs on this computer soon. Renew in a terminal: tmgr runner allow alpha',
					token: null,
				},
			],
		]);
	});

	it('stays quiet for far, missing, past and already expired entries', async () => {
		entries = [
			entry('far', new Date(now + 4 * day).toISOString()),
			entry('never', null),
			entry('past', new Date(now - day).toISOString()),
			{ ...entry('gone', new Date(now + day).toISOString()), expired: true },
		];
		await checkAllowReminders(now);
		expect(notifications()).toHaveLength(0);
	});

	it('repeats at most once per local day per workspace', async () => {
		entries = [entry('alpha', new Date(now + 2 * day).toISOString())];
		await checkAllowReminders(now);
		await checkAllowReminders(now + 6 * 3_600_000 - 1);
		expect(notifications()).toHaveLength(1);
		await checkAllowReminders(now + day);
		expect(notifications()).toHaveLength(2);
	});

	it('sends one notification per expiring workspace in a single burst on the first check', async () => {
		entries = Array.from({ length: 12 }, (_, i) =>
			entry(`ws${i}`, new Date(now + day).toISOString()),
		);
		await checkAllowReminders(now);
		expect(notifications()).toHaveLength(12);
		expect(notifications().map(([, args]) => args.body.split(' ')[1])).toEqual(
			entries.map((e) => e.workspaceCode),
		);
		await checkAllowReminders(now);
		expect(notifications()).toHaveLength(12);
	});
});
