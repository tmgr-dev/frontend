import { openTaskLink, taskLinkFailureMessage } from '../openTaskLink';

const workspaces = [
	{ id: 1, code: 'home' },
	{ id: 2, code: 'work' },
	{ id: -3, code: 'local' },
];

const makeDeps = (overrides: Record<string, unknown> = {}) => {
	const calls: string[] = [];
	const deps = {
		loadWorkspaces: jest.fn(async () => workspaces),
		currentWorkspaceId: jest.fn(() => 1),
		fetchTask: jest.fn(async () => {
			calls.push('fetch');
		}),
		focusTaskWindow: jest.fn(async () => false),
		showMain: jest.fn(async () => {
			calls.push('show');
		}),
		navigate: jest.fn(async (path: string) => {
			calls.push(`nav:${path}`);
		}),
		openModal: jest.fn((id: number) => {
			calls.push(`modal:${id}`);
		}),
		...overrides,
	};
	return { deps, calls };
};

const httpError = (status: number) => ({ response: { status } });

describe('openTaskLink', () => {
	it('opens a task of the current workspace in the modal without navigating', async () => {
		const { deps, calls } = makeDeps();
		const result = await openTaskLink(
			{ workspaceCode: 'home', taskId: 5 },
			deps,
		);
		expect(result).toEqual({ ok: true });
		expect(calls).toEqual(['fetch', 'show', 'modal:5']);
		expect(deps.fetchTask).toHaveBeenCalledWith(5, 1);
	});

	it('checks the task before switching workspace, then switches', async () => {
		const { deps, calls } = makeDeps();
		await openTaskLink({ workspaceCode: 'work', taskId: 7 }, deps);
		expect(deps.fetchTask).toHaveBeenCalledWith(7, 2);
		expect(calls).toEqual(['fetch', 'show', 'nav:/work/list', 'modal:7']);
	});

	it('skips the remote check when the current workspace is local', async () => {
		const { deps } = makeDeps({ currentWorkspaceId: jest.fn(() => -3) });
		const result = await openTaskLink(
			{ workspaceCode: 'work', taskId: 7 },
			deps,
		);
		expect(result).toEqual({ ok: true });
		expect(deps.fetchTask).not.toHaveBeenCalled();
		expect(deps.openModal).toHaveBeenCalledWith(7);
	});

	it('focuses the task window and does nothing else', async () => {
		const { deps, calls } = makeDeps({
			focusTaskWindow: jest.fn(async () => true),
		});
		const result = await openTaskLink(
			{ workspaceCode: 'work', taskId: 7 },
			deps,
		);
		expect(result).toEqual({ ok: true });
		expect(deps.focusTaskWindow).toHaveBeenCalledWith(7, 'work');
		expect(calls).toEqual(['fetch']);
	});

	it('does not switch workspace when the task is missing or forbidden', async () => {
		for (const [status, reason] of [
			[404, 'not_found'],
			[403, 'forbidden'],
		] as const) {
			const { deps, calls } = makeDeps({
				fetchTask: jest.fn(async () => {
					throw httpError(status);
				}),
			});
			const result = await openTaskLink(
				{ workspaceCode: 'work', taskId: 7 },
				deps,
			);
			expect(result).toEqual({ ok: false, reason });
			expect(calls).toEqual([]);
		}
	});

	it('reports other failures as a generic error', async () => {
		const { deps } = makeDeps({
			fetchTask: jest.fn(async () => {
				throw new Error('network');
			}),
		});
		expect(
			await openTaskLink({ workspaceCode: 'home', taskId: 1 }, deps),
		).toEqual({
			ok: false,
			reason: 'error',
		});
	});

	it('reports an unknown workspace', async () => {
		const { deps, calls } = makeDeps();
		const result = await openTaskLink(
			{ workspaceCode: 'nope', taskId: 1 },
			deps,
		);
		expect(result).toEqual({ ok: false, reason: 'workspace' });
		expect(calls).toEqual([]);
	});

	it('uses the current workspace for a link without one', async () => {
		const { deps, calls } = makeDeps();
		await openTaskLink({ workspaceCode: null, taskId: 9 }, deps);
		expect(deps.fetchTask).toHaveBeenCalledWith(9, 1);
		expect(calls).toEqual(['fetch', 'show', 'modal:9']);
	});

	it('skips the remote check for a local workspace', async () => {
		const { deps, calls } = makeDeps();
		await openTaskLink({ workspaceCode: 'local', taskId: 4 }, deps);
		expect(deps.fetchTask).not.toHaveBeenCalled();
		expect(calls).toEqual(['show', 'nav:/local/list', 'modal:4']);
	});
});

describe('taskLinkFailureMessage', () => {
	it('gives every failure a readable message', () => {
		for (const reason of [
			'workspace',
			'not_found',
			'forbidden',
			'error',
		] as const) {
			expect(taskLinkFailureMessage(reason).length).toBeGreaterThan(10);
		}
	});
});
