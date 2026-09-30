import { openTaskInWorkspace, openTaskPreferringWindow } from '../openTaskInWorkspace';

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

describe('openTaskPreferringWindow', () => {
	const setup = (currentWorkspaceId: number, windowOpen: boolean | Error) => ({
		store: makeStore(currentWorkspaceId),
		router: { push: jest.fn() },
		showMain: jest.fn(),
		focusWindow: jest.fn(() =>
			windowOpen instanceof Error
				? Promise.reject(windowOpen)
				: Promise.resolve(windowOpen),
		),
	});

	it('focuses an open task window and leaves the main window alone', async () => {
		const { store, router, showMain, focusWindow } = setup(-2, true);

		await openTaskPreferringWindow(
			{ taskId: 7, workspaceId: 1 },
			store,
			router,
			showMain,
			focusWindow,
		);

		expect(focusWindow).toHaveBeenCalledWith(7, 'cloud');
		expect(showMain).not.toHaveBeenCalled();
		expect(router.push).not.toHaveBeenCalled();
		expect(store.commit).not.toHaveBeenCalled();
	});

	it('looks up the window of a local workspace task by its code', async () => {
		const { store, router, showMain, focusWindow } = setup(1, true);

		await openTaskPreferringWindow(
			{ taskId: '3', workspaceId: -2 },
			store,
			router,
			showMain,
			focusWindow,
		);

		expect(focusWindow).toHaveBeenCalledWith(3, 'local');
		expect(store.commit).not.toHaveBeenCalled();
	});

	it('uses the current workspace code when the task has no workspace', async () => {
		const { store, router, showMain, focusWindow } = setup(-2, true);

		await openTaskPreferringWindow(
			{ taskId: 7, workspaceId: null },
			store,
			router,
			showMain,
			focusWindow,
		);

		expect(focusWindow).toHaveBeenCalledWith(7, 'local');
	});

	it('shows the main window and opens the task there when no task window is open', async () => {
		const { store, router, showMain, focusWindow } = setup(-2, false);

		await openTaskPreferringWindow(
			{ taskId: 7, workspaceId: 1 },
			store,
			router,
			showMain,
			focusWindow,
		);

		expect(showMain).toHaveBeenCalled();
		expect(router.push).toHaveBeenCalledWith('/cloud/list');
		expect(store.commit).toHaveBeenCalledWith('setCurrentTaskIdForModal', 7);
	});

	it('falls back to the main window when focusing fails', async () => {
		const { store, router, focusWindow } = setup(1, new Error('no tauri'));

		await openTaskPreferringWindow(
			{ taskId: 7, workspaceId: 1 },
			store,
			router,
			undefined,
			focusWindow,
		);

		expect(store.commit).toHaveBeenCalledWith('setCurrentTaskIdForModal', 7);
	});

	it('skips the window lookup for an unknown workspace and keeps the old behaviour', async () => {
		const { store, router, showMain, focusWindow } = setup(1, true);

		await openTaskPreferringWindow(
			{ taskId: 7, workspaceId: 99 },
			store,
			router,
			showMain,
			focusWindow,
		);

		expect(focusWindow).not.toHaveBeenCalled();
		expect(showMain).toHaveBeenCalled();
		expect(store.commit).not.toHaveBeenCalled();
	});
});
