import { openTaskInWorkspace } from '../openTaskInWorkspace';

const workspaces = [
	{ id: 1, code: 'cloud' },
	{ id: -2, code: 'local' },
];

const makeStore = (currentWorkspaceId: number | string) => ({
	state: {
		workspaces,
		workspacesById: Object.fromEntries(workspaces.map((w) => [w.id, w])),
	},
	getters: { currentWorkspaceId },
	commit: jest.fn(),
});

describe('openTaskInWorkspace', () => {
	it('navigates to the task workspace and opens the task only after the push resolves', async () => {
		const store = makeStore(-2);
		let resolvePush: () => void = () => {};
		const router = {
			push: jest.fn(
				() => new Promise<void>((resolve) => (resolvePush = resolve)),
			),
		};

		const pending = openTaskInWorkspace(
			{ taskId: 7, workspaceId: 1 },
			store,
			router,
		);
		await Promise.resolve();

		expect(router.push).toHaveBeenCalledWith('/cloud/list');
		expect(store.commit).not.toHaveBeenCalled();

		resolvePush();
		await pending;

		expect(store.commit).toHaveBeenCalledWith('setCurrentTaskIdForModal', 7);
	});

	it('opens a local task from a cloud window through the local workspace', async () => {
		const store = makeStore(1);
		const router = { push: jest.fn().mockResolvedValue(undefined) };

		await openTaskInWorkspace({ taskId: 9, workspaceId: -2 }, store, router);

		expect(router.push).toHaveBeenCalledWith('/local/list');
		expect(store.commit).toHaveBeenCalledWith('setCurrentTaskIdForModal', 9);
	});

	it('only commits when the task is in the current workspace, comparing ids numerically', async () => {
		const store = makeStore('1');
		const router = { push: jest.fn() };

		await openTaskInWorkspace({ taskId: 7, workspaceId: 1 }, store, router);

		expect(router.push).not.toHaveBeenCalled();
		expect(store.commit).toHaveBeenCalledWith('setCurrentTaskIdForModal', 7);
	});

	it('only commits when the task has no workspace', async () => {
		const store = makeStore(1);
		const router = { push: jest.fn() };

		await openTaskInWorkspace({ taskId: 7, workspaceId: null }, store, router);

		expect(router.push).not.toHaveBeenCalled();
		expect(store.commit).toHaveBeenCalledWith('setCurrentTaskIdForModal', 7);
	});

	it('does nothing for an unknown workspace', async () => {
		const store = makeStore(1);
		const router = { push: jest.fn() };

		await expect(
			openTaskInWorkspace({ taskId: 7, workspaceId: 99 }, store, router),
		).resolves.toBeUndefined();

		expect(router.push).not.toHaveBeenCalled();
		expect(store.commit).not.toHaveBeenCalled();
	});
});
