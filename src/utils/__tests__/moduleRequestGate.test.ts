import {
	moduleGateBlocks,
	moduleKeyForRequest,
	moduleOffError,
	shouldBlockRequest,
	workspaceIdInUrl,
} from '../moduleRequestGate';

describe('moduleKeyForRequest', () => {
	it.each([
		['daily-routines/tasks/count', 'daily_routines'],
		['/daily-routines/tasks', 'daily_routines'],
		['/workspaces/5/dashboard/heatmap', 'dashboard'],
		['/workspaces/5/dashboard/team-activity?x=1', 'dashboard'],
		['/workspaces/5/files', 'task.files'],
		['/tasks/9/files', 'task.files'],
		['tasks/9/pomodoro', 'pomodoro'],
		['task-relation-types', 'task.relations'],
		['/tasks/9/github/commits', 'github'],
		['/categories/3/github', 'github'],
		['/github/installations', 'github'],
		['/cursor-agents/active', 'cursor'],
		['/tasks/9/cursor-agent', 'cursor'],
		['/workspaces/5/assignable-personas', 'personas'],
		['/workspaces/5/personas', 'personas'],
		['/personas', 'personas'],
		['telegram/link/generate', 'telegram'],
		['/agent/conversations', 'ai.assistant'],
		['/ai/transcribe', 'ai.assistant'],
		['tasks/9/agent-work', 'agent_work'],
		['workspaces/5/agent-status', 'agent_work'],
		['/user/alarm-phone', 'alerts'],
		['/notify-tokens', 'alerts'],
		['pages/tree', 'pages'],
		['tasks/9/pages', 'pages'],
		['graph/related', 'graph'],
	])('%s belongs to %s', (url, key) => {
		expect(moduleKeyForRequest(url)).toBe(key);
	});

	it.each([
		'tasks',
		'/tasks/9',
		'categories',
		'/workspaces/5/statuses',
		'/user',
		'/files/12/content',
	])('%s belongs to no module', (url) => {
		expect(moduleKeyForRequest(url)).toBeNull();
	});
});

describe('shouldBlockRequest', () => {
	const entries: Record<string, { enabled?: boolean; hidden?: boolean }> = {
		dashboard: { enabled: true, hidden: true },
		pomodoro: { enabled: false },
		pages: { enabled: true },
	};
	const lookup = (key: string) => entries[key];

	it('blocks reads of a module the user hid', () => {
		expect(
			shouldBlockRequest('get', '/workspaces/5/dashboard/heatmap', lookup),
		).toBe(true);
	});
	it('lets writes through for a module only hidden for the user', () => {
		expect(
			shouldBlockRequest('post', '/workspaces/5/dashboard/refresh/x', lookup),
		).toBe(false);
	});
	it('blocks every method for a module the owner turned off', () => {
		expect(shouldBlockRequest('get', 'tasks/9/pomodoro', lookup)).toBe(true);
		expect(shouldBlockRequest('post', 'tasks/9/pomodoro', lookup)).toBe(true);
	});
	it('lets a visible module and unknown modules through', () => {
		expect(shouldBlockRequest('get', 'pages/tree', lookup)).toBe(false);
		expect(shouldBlockRequest('get', 'graph/related', lookup)).toBe(false);
		expect(shouldBlockRequest('get', 'tasks', lookup)).toBe(false);
	});
});

describe('shouldBlockRequest while modules are unknown', () => {
	const none = () => undefined;
	it('leaves gated reads to the server while the modules state is unknown', () => {
		expect(
			shouldBlockRequest('get', 'daily-routines/tasks/count', none, false),
		).toBe(false);
		expect(
			shouldBlockRequest('get', 'daily-routines/tasks/count', none, true),
		).toBe(false);
	});
	it('lets ungated reads and writes through', () => {
		expect(shouldBlockRequest('get', 'tasks', none, false)).toBe(false);
		expect(shouldBlockRequest('post', 'tasks/9/pomodoro', none, false)).toBe(
			false,
		);
	});
});

describe('moduleGateBlocks on a cold load (TM-728)', () => {
	const gate = (
		entries: Record<string, { enabled?: boolean; hidden?: boolean }>,
		load: (state: { known: boolean }) => Promise<unknown>,
	) => {
		const state = { known: false };
		const loadModules = jest.fn(() => load(state));
		const deps = {
			entryFor: () => (key: string) => (state.known ? entries[key] : undefined),
			sameWorkspace: () => true,
			modulesKnown: () => state.known,
			loadModules,
		};
		return { deps, loadModules };
	};
	const loaded = async (state: { known: boolean }) => {
		state.known = true;
	};

	it('waits for the modules load and lets an enabled module through', async () => {
		const { deps, loadModules } = gate(
			{ daily_routines: { enabled: true } },
			loaded,
		);
		await expect(
			moduleGateBlocks('get', 'daily-routines/tasks?from=1', deps),
		).resolves.toBeNull();
		expect(loadModules).toHaveBeenCalledTimes(1);
	});

	it('still blocks a module that is really off once modules load', async () => {
		const { deps } = gate({ daily_routines: { enabled: false } }, loaded);
		await expect(
			moduleGateBlocks('get', 'daily-routines/tasks', deps),
		).resolves.toBe('daily_routines');
	});

	it('passes through to the server when modules stay unknown', async () => {
		const { deps, loadModules } = gate(
			{ daily_routines: { enabled: false } },
			async () => undefined,
		);
		await expect(
			moduleGateBlocks('get', 'daily-routines/tasks', deps),
		).resolves.toBeNull();
		expect(loadModules).toHaveBeenCalledTimes(1);
	});

	it('passes through when the modules load fails', async () => {
		const { deps } = gate({ daily_routines: { enabled: false } }, () =>
			Promise.reject(new Error('network')),
		);
		await expect(
			moduleGateBlocks('get', 'daily-routines/tasks', deps),
		).resolves.toBeNull();
	});

	it('never loads modules for ungated requests, the modules fetch included', async () => {
		const { deps, loadModules } = gate({}, loaded);
		await expect(
			moduleGateBlocks('get', '/workspaces/5/modules', deps),
		).resolves.toBeNull();
		await expect(moduleGateBlocks('get', 'tasks', deps)).resolves.toBeNull();
		expect(loadModules).not.toHaveBeenCalled();
	});

	it('does not load again when modules are already known', async () => {
		const { deps, loadModules } = gate({ pages: { enabled: true } }, loaded);
		deps.modulesKnown = () => true;
		deps.entryFor = () => (key: string) => ({ pages: { enabled: true } })[key];
		await expect(
			moduleGateBlocks('get', 'pages/tree', deps),
		).resolves.toBeNull();
		expect(loadModules).not.toHaveBeenCalled();
	});

	it('skips the gate for another workspace', async () => {
		const { deps, loadModules } = gate(
			{ dashboard: { enabled: false } },
			loaded,
		);
		deps.sameWorkspace = () => false;
		await expect(
			moduleGateBlocks('get', '/workspaces/7/dashboard/heatmap', deps),
		).resolves.toBeNull();
		expect(loadModules).not.toHaveBeenCalled();
	});
});

describe('moduleOffError', () => {
	it('looks like the server feature_disabled response', () => {
		const error: any = moduleOffError({ url: 'tasks/9/pomodoro' }, 'pomodoro');
		expect(error.response.status).toBe(403);
		expect(error.response.data.error).toBe('feature_disabled');
		expect(error.response.data.feature).toBe('pomodoro');
		expect(error.config.url).toBe('tasks/9/pomodoro');
	});
});

describe('workspaceIdInUrl', () => {
	it('reads the workspace of a workspace-scoped url', () => {
		expect(workspaceIdInUrl('/workspaces/7/dashboard/heatmap')).toBe('7');
		expect(workspaceIdInUrl('workspaces/7/files?x=1')).toBe('7');
		expect(workspaceIdInUrl('tasks/7')).toBeNull();
	});
});
