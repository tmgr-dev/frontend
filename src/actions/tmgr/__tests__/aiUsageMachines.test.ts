const get = jest.fn();
const del = jest.fn();
jest.mock('@/plugins/axios', () => ({
	__esModule: true,
	default: {
		get: (...a: any[]) => get(...a),
		delete: (...a: any[]) => del(...a),
	},
}));

import { fetchDaily, fetchMachines, removeMachine } from '../aiUsageMachines';

beforeEach(() => {
	get.mockReset();
	del.mockReset();
});

describe('aiUsageMachines actions', () => {
	it('fetchMachines unwraps the machines list', async () => {
		get.mockResolvedValue({ data: { machines: [{ machine_id: 'm1' }] } });
		expect(await fetchMachines()).toEqual([{ machine_id: 'm1' }]);
		expect(get).toHaveBeenCalledWith('ai-usage/machines');
	});

	it('fetchMachines tolerates a data envelope and bad bodies', async () => {
		get.mockResolvedValueOnce({ data: { data: { machines: [{ a: 1 }] } } });
		expect(await fetchMachines()).toEqual([{ a: 1 }]);
		get.mockResolvedValueOnce({ data: null });
		expect(await fetchMachines()).toEqual([]);
	});

	it('fetchDaily sends snake_case params and only the set filters', async () => {
		get.mockResolvedValue({ data: { rows: [{ day: 'd' }], truncated: true } });
		const res = await fetchDaily({
			from: '2026-11-01',
			to: '2026-11-21',
			machineId: 'm1',
			agent: 'codex',
			model: 'x',
			taskId: 5,
		});
		expect(res).toEqual({ rows: [{ day: 'd' }], truncated: true });
		expect(get).toHaveBeenCalledWith('ai-usage/daily', {
			params: {
				from: '2026-11-01',
				to: '2026-11-21',
				machine_id: 'm1',
				agent: 'codex',
				model: 'x',
				task_id: 5,
			},
		});
	});

	it('fetchDaily omits unset filters', async () => {
		get.mockResolvedValue({ data: {} });
		const res = await fetchDaily({ from: 'a', to: 'b' });
		expect(res).toEqual({ rows: [], truncated: false });
		expect(get).toHaveBeenCalledWith('ai-usage/daily', {
			params: { from: 'a', to: 'b' },
		});
	});

	it('removeMachine deletes by id', async () => {
		del.mockResolvedValue({ status: 204 });
		await removeMachine('abc-1');
		expect(del).toHaveBeenCalledWith('ai-usage/machines/abc-1');
	});
});
