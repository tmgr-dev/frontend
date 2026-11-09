jest.mock('@/plugins/axios', () => ({
	__esModule: true,
	default: { get: jest.fn() },
}));

import axios from '@/plugins/axios';
import { getArchivedDailyTasksCount } from '../daily-tasks';

describe('getArchivedDailyTasksCount', () => {
	it('unwraps the count from the response envelope', async () => {
		(axios.get as jest.Mock).mockResolvedValueOnce({
			data: { data: { count: 4 } },
		});
		expect(await getArchivedDailyTasksCount()).toBe(4);
		expect(axios.get).toHaveBeenCalledWith(
			'daily-routines/tasks/archived/count',
		);
	});
});
