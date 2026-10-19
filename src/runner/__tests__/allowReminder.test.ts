import type { AllowedEntry } from '@/utils/agentApprovalPolicy';

jest.mock('@tauri-apps/api/core', () => ({ invoke: jest.fn() }));
jest.mock('@/utils/desktop', () => ({ isDesktopApp: () => false }));

import { allowCommand, dueReminders, localDay } from '../allowReminder';

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
