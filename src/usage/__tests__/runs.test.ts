const get = jest.fn();
jest.mock('@/plugins/axios', () => ({ __esModule: true, default: { get: (...a: unknown[]) => get(...a) } }));

import { fetchRuns, taskInfoFor } from '../runs';

const run = (id: number, extra: object = {}) => ({
	id,
	task_id: 40 + id,
	status: 'succeeded',
	started_at: '2026-10-10T10:00:00Z',
	ended_at: null,
	duration_seconds: 60,
	session_id: 's',
	branch: 'feat/x',
	task: { id: 40 + id, key: `TM-${id}`, title: `Task ${id}` },
	...extra,
});

describe('fetchRuns', () => {
	const now = new Date('2026-10-15T12:00:00Z');
	beforeEach(() => get.mockReset());

	it('maps runs and task info across workspaces and skips failing ones', async () => {
		get.mockImplementation(async (url: string) => {
			if (url === 'workspaces/1/agent-work')
				return { data: { data: { runs: [run(1)], next_cursor: null } } };
			throw new Error('forbidden');
		});
		const out = await fetchRuns(
			[
				{ id: 1, code: 'demo' },
				{ id: 2, code: 'broken' },
			],
			now,
		);
		expect(out.runs).toEqual([
			{
				id: 1,
				task_id: 41,
				workspace: 'demo',
				status: 'succeeded',
				started_at: '2026-10-10T10:00:00Z',
				ended_at: null,
				duration_seconds: 60,
				session_id: 's',
				branch: 'feat/x',
			},
		]);
		expect(out.tasks['demo/41']).toEqual({
			key: 'TM-1',
			title: 'Task 1',
			path: '/demo/tasks/41',
		});
		const params = get.mock.calls[0][1].params;
		expect(params.limit).toBe(200);
		expect(params.until).toBe('2026-10-15T12:00:00.000Z');
		expect(params.since).toBe('2026-09-12T12:00:00.000Z');
	});

	it('follows cursors up to 10 pages', async () => {
		get.mockImplementation(async (_u: string, cfg: any) => ({
			data: { data: { runs: [run(Number(cfg.params.cursor ?? 0) + 1)], next_cursor: String((cfg.params.cursor ?? 0) + 1) } },
		}));
		const out = await fetchRuns([{ id: 1, code: 'demo' }], now);
		expect(get).toHaveBeenCalledTimes(10);
		expect(out.runs).toHaveLength(10);
		expect(get.mock.calls[1][1].params.cursor).toBe('1');
	});

	it('stops when there is no cursor and falls back to a bare task id', async () => {
		get.mockResolvedValue({ data: { data: [run(3, { task: undefined })] } });
		const out = await fetchRuns([{ id: 1, code: 'demo' }], now);
		expect(get).toHaveBeenCalledTimes(1);
		expect(out.tasks['demo/43'].key).toBe('#43');
		expect(taskInfoFor(out.tasks, 'demo/99')).toEqual({
			key: '#99',
			title: null,
			path: '/demo/tasks/99',
		});
	});
});
