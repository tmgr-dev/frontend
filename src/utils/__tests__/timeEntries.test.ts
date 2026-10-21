import type { TaskTimeEntry } from '@/actions/tmgr/timeEntries';
import {
	shouldLoadTimeEntries,
	timeEntryRows,
	timeEntrySourceLabel,
} from '../timeEntries';

const entry = (over: Partial<TaskTimeEntry> = {}): TaskTimeEntry => ({
	id: 1,
	user_id: 10,
	user_name: 'Ann',
	source: 'attach',
	started_at: '2026-10-21T10:00:00Z',
	ended_at: '2026-10-21T10:05:30Z',
	seconds: 330,
	counted_seconds: 330,
	created_at: '2026-10-21T10:06:00Z',
	...over,
});

describe('timeEntrySourceLabel', () => {
	it('labels attach as a terminal session', () => {
		expect(timeEntrySourceLabel('attach')).toBe('Terminal session');
	});

	it('falls back to the raw source', () => {
		expect(timeEntrySourceLabel('other')).toBe('other');
	});
});

describe('timeEntryRows', () => {
	it('renders date, range, counted duration and source', () => {
		const [row] = timeEntryRows([entry()], 10);
		expect(row).toEqual({
			id: 1,
			date: '21 Oct 2026',
			range: '10:00–10:05',
			counted: '5m 30s',
			source: 'Terminal session',
			user: null,
			partial: false,
		});
	});

	it('shows the user name only for other users', () => {
		expect(timeEntryRows([entry()], 10)[0].user).toBeNull();
		expect(timeEntryRows([entry()], 99)[0].user).toBe('Ann');
	});

	it('flags entries partly covered by the timer', () => {
		const [row] = timeEntryRows(
			[entry({ counted_seconds: 100, seconds: 330 })],
			10,
		);
		expect(row.partial).toBe(true);
		expect(row.counted).toBe('1m 40s');
	});

	it('returns nothing for no entries', () => {
		expect(timeEntryRows([], 10)).toEqual([]);
	});
});

describe('shouldLoadTimeEntries', () => {
	it('loads for cloud tasks', () => {
		expect(shouldLoadTimeEntries(5, 3)).toBe(true);
		expect(shouldLoadTimeEntries(5, undefined)).toBe(true);
	});

	it('does not load for local workspaces', () => {
		expect(shouldLoadTimeEntries(5, -2)).toBe(false);
	});

	it('does not load without a task id', () => {
		expect(shouldLoadTimeEntries(NaN, 3)).toBe(false);
		expect(shouldLoadTimeEntries(0, 3)).toBe(false);
	});
});
