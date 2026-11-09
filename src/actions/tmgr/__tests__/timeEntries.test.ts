jest.mock('@/plugins/axios', () => ({
	__esModule: true,
	default: { get: jest.fn(), post: jest.fn() },
}));

import axios from '@/plugins/axios';
import { createTimeEntries, getTaskTimeEntries } from '../timeEntries';

const ENTRY = {
	id: 5,
	user_id: 1,
	user_name: 'Ann',
	source: 'attach',
	started_at: '2026-10-21T10:00:00Z',
	ended_at: '2026-10-21T10:05:00Z',
	seconds: 240,
	counted_seconds: 180,
	created_at: '2026-10-21T10:06:00Z',
};

describe('getTaskTimeEntries', () => {
	beforeEach(() => jest.clearAllMocks());

	it('GETs the task time entries and unwraps the envelope', async () => {
		(axios.get as jest.Mock).mockResolvedValue({ data: { data: [ENTRY] } });
		await expect(getTaskTimeEntries(7)).resolves.toEqual([ENTRY]);
		expect(axios.get).toHaveBeenCalledWith('tasks/7/time-entries');
	});

	it('returns an empty list when the payload has no array', async () => {
		(axios.get as jest.Mock).mockResolvedValue({ data: { data: null } });
		await expect(getTaskTimeEntries(7)).resolves.toEqual([]);
	});
});

describe('createTimeEntries', () => {
	beforeEach(() => jest.clearAllMocks());

	const entry = {
		client_interval_id: 'act-0123456789abcdef',
		source: 'activity' as const,
		started_at: '2026-10-21T10:00:00Z',
		ended_at: '2026-10-21T10:20:00Z',
		seconds: 1200,
	};

	it('POSTs the entries and returns the per-entry results', async () => {
		const results = [
			{ client_interval_id: entry.client_interval_id, status: 'created' },
		];
		(axios.post as jest.Mock).mockResolvedValue({ data: { entries: results } });
		await expect(createTimeEntries(7, [entry])).resolves.toEqual(results);
		expect(axios.post).toHaveBeenCalledWith('tasks/7/time-entries', {
			entries: [entry],
		});
	});

	it('also reads results wrapped in a data envelope', async () => {
		const results = [{ status: 'duplicate' }];
		(axios.post as jest.Mock).mockResolvedValue({
			data: { data: { entries: results } },
		});
		await expect(createTimeEntries(7, [entry])).resolves.toEqual(results);
	});

	it('returns an empty list when the payload has no entries', async () => {
		(axios.post as jest.Mock).mockResolvedValue({ data: {} });
		await expect(createTimeEntries(7, [entry])).resolves.toEqual([]);
	});
});
