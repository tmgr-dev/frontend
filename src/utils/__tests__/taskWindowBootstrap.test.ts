import { bootstrapTaskWindow } from '../taskWindowBootstrap';

const workspaces = [
	{ id: 56, code: 'tmgrdev' },
	{ id: -7, code: 'current-project' },
];

const setup = (loaded = workspaces) => {
	const calls: string[] = [];
	const commits: [string, unknown][] = [];
	const store = {
		state: { workspaces: loaded as typeof workspaces | null },
		commit: (mutation: string, payload?: unknown) => {
			calls.push(`commit:${mutation}`);
			commits.push([mutation, payload]);
			if (mutation === 'setWorkspaces') store.state.workspaces = payload as any;
		},
	};
	const deps = {
		getWorkspaces: jest.fn(async () => workspaces),
		syncActiveLocalWorkspace: jest.fn(async (id: number) => {
			calls.push(`sync:${id}`);
		}),
		getUserFeatureToggles: jest.fn(async () => ({ workspaces: { enabled: true } })),
		getWorkspaceFeatureToggles: jest.fn(async (id: number) => {
			calls.push(`toggles:${id}`);
			return { 'task.files': { enabled: true }, 'task.checkpoints': { enabled: true } };
		}),
	};
	return { store, deps, calls, commits };
};

describe('bootstrapTaskWindow', () => {
	test.each([
		['a cloud task', 'tmgrdev', 56],
		['a local workspace task', 'current-project', -7],
	])('loads the feature toggles of the workspace of %s before the form opens', async (_, code, id) => {
		const { store, deps, calls, commits } = setup();
		const workspace = await bootstrapTaskWindow(store, deps, code, 60);
		expect(workspace?.id).toBe(id);
		expect(deps.getWorkspaceFeatureToggles).toHaveBeenCalledWith(id);
		expect(commits).toContainEqual([
			'featureToggles/setWorkspaceToggles',
			{ 'task.files': { enabled: true }, 'task.checkpoints': { enabled: true } },
		]);
		expect(commits).toContainEqual([
			'featureToggles/setUserToggles',
			{ workspaces: { enabled: true } },
		]);
		expect(calls.indexOf(`sync:${id}`)).toBeLessThan(calls.indexOf(`toggles:${id}`));
		expect(calls[calls.length - 1]).toBe('commit:setCurrentTaskIdForModal');
	});

	test('loads the workspaces when this window has none yet', async () => {
		const { store, deps } = setup([] as any);
		await bootstrapTaskWindow(store, deps, 'tmgrdev', 1);
		expect(deps.getWorkspaces).toHaveBeenCalled();
	});

	test('opens nothing for an unknown workspace or task id', async () => {
		const { store, deps, commits } = setup();
		expect(await bootstrapTaskWindow(store, deps, 'nope', 1)).toBeNull();
		expect(await bootstrapTaskWindow(store, deps, 'tmgrdev', 0)).toBeNull();
		expect(commits.some(([m]) => m === 'setCurrentTaskIdForModal')).toBe(false);
	});
});
