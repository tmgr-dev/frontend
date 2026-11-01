import { CloudError, createCloudApi, firstActiveStatusId } from '../cloud';
import { createMoveDeps, prepareTarget } from '../moveDeps';

const http = () => {
	const calls: any[] = [];
	return {
		calls,
		get: jest.fn(async (url: string, config?: any) => {
			calls.push({ method: 'GET', url, config });
			return { data: { data: [] } };
		}),
		post: jest.fn(async (url: string, body?: any, config?: any) => {
			calls.push({ method: 'POST', url, body, config });
			return { data: { data: { id: 42 } } };
		}),
	};
};

describe('createCloudApi', () => {
	it('posts a routine with the target workspace header', async () => {
		const h = http();
		const api = createCloudApi(h);
		const created = await api.createRoutine('daily-routines/tasks', {
			title: 'A',
			workspace_id: 7,
		});
		expect(created).toEqual({ id: 42 });
		expect(h.calls[0]).toMatchObject({
			method: 'POST',
			url: 'daily-routines/tasks',
			body: { title: 'A', workspace_id: 7 },
			config: { headers: { 'X-Workspace-Id': '7' } },
		});
	});

	it('posts a task with the idempotency key', async () => {
		const h = http();
		await createCloudApi(h).createTask(
			{ title: 'A', workspace_id: 7, status_id: 3 },
			'key-1',
		);
		expect(h.calls[0]).toMatchObject({
			url: 'tasks',
			config: {
				headers: { 'Idempotency-Key': 'key-1', 'X-Workspace-Id': '7' },
			},
		});
	});

	it('reads statuses and categories of the target workspace from the server client', async () => {
		const h = http();
		const api = createCloudApi(h);
		await api.statuses(7);
		await api.categories(7);
		expect(h.calls.map((c) => c.url)).toEqual([
			'workspaces/7/statuses',
			'project_categories?all&workspace_id=7',
		]);
	});

	it('maps a 401 to an auth error after one session refresh and replay', async () => {
		const post = jest.fn(async () => {
			throw { response: { status: 401, data: { message: 'Unauthenticated' } } };
		});
		const refreshSession = jest.fn(async () => {});
		const api = createCloudApi({ get: jest.fn(), post }, { refreshSession });
		await expect(
			api.createRoutine('daily-routines/tasks', {
				title: 'A',
				workspace_id: 7,
			}),
		).rejects.toMatchObject({ kind: 'auth' });
		expect(post).toHaveBeenCalledTimes(2);
		expect(refreshSession).toHaveBeenCalledTimes(1);
	});

	it('maps a missing response to a network error and a status to the server message', async () => {
		const api = createCloudApi({
			get: jest.fn(),
			post: jest
				.fn()
				.mockRejectedValueOnce(new Error('Network Error'))
				.mockRejectedValueOnce({
					response: { status: 422, data: { message: 'No status' } },
				}),
		});
		const body = { title: 'A', workspace_id: 7 };
		await expect(api.createRoutine('x', body)).rejects.toMatchObject({
			kind: 'network',
		});
		const error = await api.createRoutine('x', body).catch((e) => e);
		expect(error).toBeInstanceOf(CloudError);
		expect(error).toMatchObject({ kind: 'rejected', message: 'No status' });
	});
});

describe('firstActiveStatusId', () => {
	it('takes the active status with the lowest order, then id', () => {
		expect(
			firstActiveStatusId([
				{ id: 5, pivot: { is_active: 0, order: 0 } },
				{ id: 4, pivot: { is_active: 1, order: 2 } },
				{ id: 3, pivot: { is_active: 1, order: 1 } },
				{ id: 2, pivot: { is_active: 1, order: 1 } },
			]),
		).toBe(2);
	});

	it('returns null when no status is active', () => {
		expect(
			firstActiveStatusId([{ id: 1, pivot: { is_active: 0, order: 1 } }]),
		).toBeNull();
		expect(firstActiveStatusId([])).toBeNull();
	});
});

describe('createMoveDeps', () => {
	const routine = {
		id: 1000000001,
		title: 'Water plants',
		description: null,
		category: null,
		approximatelyTime: null,
		pattern: null,
		scheduledDate: null,
		scheduledTime: null,
		hasHistory: false,
	};
	const api = () => ({
		statuses: jest.fn(),
		categories: jest.fn(),
		createRoutine: jest.fn(async () => ({ id: 1 })),
		createTask: jest.fn(async () => ({ id: 2 })),
	});

	it('creates a cloud routine through the routine endpoints', async () => {
		const a = api();
		const deps = createMoveDeps(a, async () => {});
		await deps.create(
			routine,
			'routines',
			{ workspaceId: 7, categoryId: null, statusId: null },
			'k',
		);
		expect(a.createRoutine).toHaveBeenCalledWith('daily-routines/tasks', {
			title: 'Water plants',
			workspace_id: 7,
		});
		expect(a.createTask).not.toHaveBeenCalled();
	});

	it('converts to a cloud task with the status and key', async () => {
		const a = api();
		const deps = createMoveDeps(a, async () => {});
		await deps.create(
			routine,
			'tasks',
			{ workspaceId: 7, categoryId: 12, statusId: 3 },
			'k',
		);
		expect(a.createTask).toHaveBeenCalledWith(
			{
				title: 'Water plants',
				workspace_id: 7,
				status_id: 3,
				project_category_id: 12,
			},
			'k',
		);
	});

	it('refuses to start a conversion when the workspace has no active status', async () => {
		const a = api();
		a.statuses.mockResolvedValue([
			{ id: 1, pivot: { is_active: 0, order: 1 } },
		]);
		await expect(prepareTarget(a, 'tasks', 7, null)).rejects.toThrow(
			'no active status',
		);
	});
});
