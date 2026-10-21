jest.mock('@/plugins/axios', () => ({
	__esModule: true,
	default: { get: jest.fn() },
}));

import axios from '@/plugins/axios';
import { getTaskTimeEntries } from '../timeEntries';

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
