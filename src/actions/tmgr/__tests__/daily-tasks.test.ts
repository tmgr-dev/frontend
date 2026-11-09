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

describe('scoped writes', () => {
	const { updateDailyTask, deleteDailyTask, getDailyTask } =
		jest.requireActual('../daily-tasks');
	const ax = axios as any;
	beforeEach(() => {
		ax.put = jest.fn().mockResolvedValue({ data: { data: {} } });
		ax.delete = jest.fn().mockResolvedValue({ data: {} });
		ax.get = jest.fn().mockResolvedValue({ data: { data: {} } });
	});

	it('sends scope, occurrence_date, op_id and base_version on update', async () => {
		await updateDailyTask(
			5,
			{ title: 'T' },
			{
				scope: 'following',
				occurrenceDate: '2026-11-10',
				opId: 'u1',
				baseVersion: 2,
			},
		);
		expect(ax.put.mock.calls[0][1]).toMatchObject({
			scope: 'following',
			occurrence_date: '2026-11-10',
			op_id: 'u1',
			base_version: 2,
		});
	});

	it('sends none of the scope fields when no options are given', async () => {
		await updateDailyTask(5, { title: 'T' });
		const body = ax.put.mock.calls[0][1];
		expect(body).not.toHaveProperty('scope');
		expect(body).not.toHaveProperty('base_version');
	});

	it('passes scope and date on delete, and none for a whole delete', async () => {
		await deleteDailyTask(5, { scope: 'single', occurrenceDate: '2026-11-10' });
		expect(ax.delete).toHaveBeenCalledWith('daily-routines/tasks/5', {
			params: { scope: 'single', occurrence_date: '2026-11-10' },
		});
		await deleteDailyTask(5);
		expect(ax.delete).toHaveBeenLastCalledWith(
			'daily-routines/tasks/5',
			undefined,
		);
	});

	it('loads the version governing a date', async () => {
		await getDailyTask(5, '2026-10-01');
		expect(ax.get).toHaveBeenCalledWith('daily-routines/tasks/5', {
			params: { date: '2026-10-01' },
		});
	});

	it('propagates a 409 from a stale base_version', async () => {
		const err = { response: { status: 409 } };
		ax.put.mockRejectedValueOnce(err);
		jest.spyOn(console, 'error').mockImplementation(() => {});
		await expect(
			updateDailyTask(5, { title: 'T' }, { scope: 'all', baseVersion: 1 }),
		).rejects.toBe(err);
	});
});
