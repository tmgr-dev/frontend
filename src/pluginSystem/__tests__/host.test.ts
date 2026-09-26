import { createDomainEvents, type DomainEvent } from '@/utils/domainEvents';
import variant from '@jitl/quickjs-wasmfile-release-sync';
import { MessageChannel } from 'node:worker_threads';
import {
	newQuickJSWASMModuleFromVariant,
	type QuickJSWASMModule,
} from 'quickjs-emscripten-core';
import type { DataApi } from '../broker';
import {
	createPluginHost,
	type PluginHostState,
	type PluginPackage,
} from '../host';
import { parseManifest } from '../manifest';
import { runPluginWorker } from '../workerRuntime';

let quickjs: Promise<QuickJSWASMModule>;
beforeAll(() => {
	quickjs = newQuickJSWASMModuleFromVariant({
		...variant,
		importModuleLoader: async () =>
			require('@jitl/quickjs-wasmfile-release-sync/emscripten-module'),
	});
});

const endpoint = () => {
	const { port1, port2 } = new MessageChannel();
	const wrap = (port: typeof port1) => ({
		postMessage: (message: unknown) => port.postMessage(message),
		addEventListener: (
			_: 'message',
			listener: (event: { data: any }) => void,
		) => port.on('message', (data) => listener({ data })),
	});
	runPluginWorker(wrap(port2) as any, () => quickjs);
	return { ...(wrap(port1) as any), terminate: () => port1.close() };
};

const LOCAL = {
	id: -7,
	code: 'local-notes',
	name: 'Notes',
	kind: 'local' as const,
};

const pkg = (
	id: string,
	code: string,
	permissions: any[] = [],
	contributes: any = {},
	source: 'builtin' | 'folder' = 'builtin',
): PluginPackage => ({
	manifest: parseManifest({
		id,
		name: id,
		version: '1.0.0',
		engines: { tmgr: '^1.0' },
		permissions,
		contributes,
	}),
	code,
	source,
});

const setup = (
	packages: PluginPackage[],
	api: Partial<DataApi> = {},
	windows: {
		open?: (...args: any[]) => Promise<void>;
		close?: (pluginId: string) => Promise<void>;
	} = {},
	blocked: Record<string, string> = {},
) => {
	const state: PluginHostState = {
		workspace: null,
		safeMode: false,
		plugins: {},
		statusBar: {},
		revision: 0,
		revisions: {},
	};
	const events = createDomainEvents();
	const notices: string[] = [];
	const enabled = new Map<string, boolean>();
	let current: number | null = LOCAL.id;
	const host = createPluginHost({
		state,
		packages: async () => packages,
		createEndpoint: endpoint,
		api: () => api as DataApi,
		subscribe: (handler) => events.on(handler),
		enabled: {
			get: (pluginId, workspaceId) => enabled.get(`${pluginId}@${workspaceId}`),
			set: (pluginId, workspaceId, value) =>
				enabled.set(`${pluginId}@${workspaceId}`, value),
		},
		settings: { get: () => undefined, set: () => undefined },
		notify: (title, message) => notices.push(`${title}: ${message}`),
		currentWorkspaceId: () => current,
		windows: {
			open: windows.open ?? (async () => undefined),
			close: windows.close ?? (async () => undefined),
		},
		blocked: (pluginId) => blocked[pluginId] ?? null,
		cpuMs: 100,
		wallMs: 2000,
	});
	return { host, state, events, notices, leave: () => (current = 56) };
};

const flush = () => new Promise((resolve) => setTimeout(resolve, 50));

it('runs enabled built-in plugins in a local workspace only, and folder plugins only when enabled', async () => {
	const { host, state } = setup([
		pkg(
			'tmgr.a',
			`tmgr.commands.register('tmgr.a.hi', () => 'hi from a');`,
			[],
			{ commands: [{ id: 'tmgr.a.hi', title: 'Hi' }] },
		),
		pkg('dev.b', '', [], {}, 'folder'),
	]);
	await host.load();
	await host.activate({ ...LOCAL, kind: 'cloud' });
	expect(state.plugins['tmgr.a'].status).toBe('stopped');

	await host.activate(LOCAL);
	expect(state.plugins['tmgr.a'].status).toBe('running');
	expect(state.plugins['dev.b'].status).toBe('stopped');
	expect(host.commands()).toEqual([
		{ pluginId: 'tmgr.a', id: 'tmgr.a.hi', title: 'Hi' },
	]);
	expect(await host.runCommand('tmgr.a', 'tmgr.a.hi')).toBe('hi from a');

	await host.setEnabled('dev.b', true);
	expect(state.plugins['dev.b'].status).toBe('running');
	await host.setEnabled('tmgr.a', false);
	expect(state.plugins['tmgr.a'].status).toBe('stopped');
	host.dispose();
});

it('starts nothing in safe mode', async () => {
	const { host, state } = setup([pkg('tmgr.a', '')]);
	state.safeMode = true;
	await host.load();
	await host.activate(LOCAL);
	expect(state.plugins['tmgr.a'].status).toBe('stopped');
	host.dispose();
});

it('delivers workspace events to subscribers, but not their own writes or other workspaces', async () => {
	const stored: unknown[] = [];
	const { host, events } = setup(
		[
			pkg(
				'tmgr.watch',
				`tmgr.events.on('timer.stopped', (e) => tmgr.storage.set('last', e.taskId));`,
				['time:read'],
			),
			pkg(
				'tmgr.blind',
				`tmgr.events.on('timer.stopped', () => tmgr.storage.set('blind', 1)).catch(() => {});`,
				[],
			),
		],
		{
			storageSet: async (key: string, json: string) =>
				void stored.push([key, json]),
		},
	);
	await host.load();
	await host.activate(LOCAL);
	const event = (extra: Partial<DomainEvent> = {}) =>
		({
			type: 'timer.stopped',
			workspaceId: LOCAL.id,
			taskId: 1,
			task: {},
			...extra,
		} as DomainEvent);
	events.emit(event());
	events.emit(event({ taskId: 2, workspaceId: 56 }));
	events.emit(event({ taskId: 3, actor: 'plugin:tmgr.watch' }));
	events.emit(event({ taskId: 4, actor: 'plugin:tmgr.other' }));
	await flush();
	expect(stored).toEqual([
		['last', '1'],
		['last', '4'],
	]);
	host.dispose();
});

it('shows status bar items while the plugin runs and badges it provides', async () => {
	const { host, state } = setup([
		pkg(
			'tmgr.bar',
			`tmgr.ui.setStatusBarItem('total', { text: '2h over', tooltip: 'Across 3 tasks' });
			 tmgr.ui.provideBadges('over', (tasks) => Object.fromEntries(tasks.map((t) => [t.id, { text: t.id + '!', color: 'red' }])));`,
			['tasks:read'],
			{ statusBarItems: [{ id: 'total' }], boardCardBadges: [{ id: 'over' }] },
		),
	]);
	await host.load();
	await host.activate(LOCAL);
	await flush();
	expect(Object.values(state.statusBar)).toEqual([
		{
			pluginId: 'tmgr.bar',
			pluginName: 'tmgr.bar',
			itemId: 'total',
			text: '2h over',
			tooltip: 'Across 3 tasks',
			command: null,
		},
	]);
	expect(await host.badges([{ id: 5 }])).toEqual({
		5: [{ pluginId: 'tmgr.bar', text: '5!', color: 'red', tooltip: null }],
	});
	await host.setEnabled('tmgr.bar', false);
	expect(state.statusBar).toEqual({});
	host.dispose();
});

it('renders declared pages through the sanitiser', async () => {
	const { host } = setup([
		pkg(
			'tmgr.page',
			`tmgr.ui.providePage('report', () => ({ type: 'stack', children: [{ type: 'heading', text: 'Report' }, { type: 'script', src: 'x' }] }));`,
			[],
			{ views: [{ id: 'report', title: 'Report' }] },
		),
	]);
	await host.load();
	await host.activate(LOCAL);
	await flush();
	expect(host.pages()).toEqual([
		{ pluginId: 'tmgr.page', id: 'report', title: 'Report' },
	]);
	expect(await host.renderPage('tmgr.page', 'report')).toEqual({
		type: 'stack',
		direction: 'column',
		children: [{ type: 'heading', text: 'Report', level: 2 }],
	});
	host.dispose();
});

it('turns a plugin off after three timeouts and tells the user', async () => {
	const { host, state, notices } = setup([
		pkg(
			'tmgr.spin',
			`tmgr.commands.register('tmgr.spin.go', () => { while (true) {} });`,
			[],
			{
				commands: [{ id: 'tmgr.spin.go', title: 'Spin' }],
			},
		),
	]);
	await host.load();
	await host.activate(LOCAL);
	for (let i = 0; i < 3; i++) {
		while (state.plugins['tmgr.spin'].status === 'starting') await flush();
		await host.runCommand('tmgr.spin', 'tmgr.spin.go').catch(() => undefined);
	}
	expect(state.plugins['tmgr.spin'].status).toBe('crashed');
	expect(notices).toEqual([
		expect.stringMatching(
			/^Plugin tmgr.spin was turned off: It failed 3 times/,
		),
	]);
	expect(await host.runCommand('tmgr.spin', 'tmgr.spin.go')).toBeUndefined();
	host.dispose();
});

it('marks a plugin whose script throws as failed', async () => {
	const { host, state } = setup([pkg('tmgr.bad', 'throw new Error("broken")')]);
	await host.load();
	await host.activate(LOCAL);
	expect(state.plugins['tmgr.bad']).toMatchObject({
		status: 'failed',
		error: expect.stringContaining('broken'),
	});
	host.dispose();
});

it('marks a plugin whose worker was killed as crashed at once', async () => {
	const { host, state } = setup([pkg('tmgr.silent', '')]);
	await host.load();
	const silent = {
		postMessage: (message: any) =>
			message.type === 'start' && setTimeout(() => undefined, 0),
		addEventListener: () => undefined,
		terminate: () => undefined,
	};
	const hostWithSilent = createPluginHost({
		state,
		packages: async () => [pkg('tmgr.silent', '')],
		createEndpoint: () => silent as any,
		api: () => ({} as DataApi),
		subscribe: () => () => undefined,
		enabled: { get: () => undefined, set: () => undefined },
		settings: { get: () => undefined, set: () => undefined },
		notify: () => undefined,
		currentWorkspaceId: () => LOCAL.id,
	});
	host.dispose();
	await hostWithSilent.load();
	jest.useFakeTimers();
	const activating = hostWithSilent.activate(LOCAL);
	jest.advanceTimersByTime(10_001);
	jest.useRealTimers();
	await activating;
	expect(state.plugins['tmgr.silent'].status).toBe('crashed');
	hostWithSilent.dispose();
});

it('gives task snapshots in events only to plugins with tasks:read', async () => {
	const seen: unknown[] = [];
	const { host, events } = setup(
		[
			pkg(
				'tmgr.timeonly',
				`tmgr.events.on('timer.stopped', (e) => tmgr.storage.set('e', e));`,
				['time:read'],
			),
		],
		{
			storageSet: async (_key: string, json: string) =>
				void seen.push(JSON.parse(json)),
		},
	);
	await host.load();
	await host.activate(LOCAL);
	events.emit({
		type: 'timer.stopped',
		workspaceId: LOCAL.id,
		taskId: 1,
		task: { title: 'Secret' },
	} as DomainEvent);
	await flush();
	expect(seen).toEqual([
		{ type: 'timer.stopped', workspaceId: LOCAL.id, taskId: 1 },
	]);
	host.dispose();
});

it('coalesces a flood of refresh requests into one redraw', async () => {
	const { host, state } = setup([
		pkg(
			'tmgr.chatty',
			`tmgr.commands.register('tmgr.chatty.go', async () => { for (let i = 0; i < 10; i++) await tmgr.ui.refresh('page', 'report'); });`,
			[],
			{
				commands: [{ id: 'tmgr.chatty.go', title: 'Go' }],
				views: [{ id: 'report', title: 'Report' }],
			},
		),
	]);
	await host.load();
	await host.activate(LOCAL);
	await host.runCommand('tmgr.chatty', 'tmgr.chatty.go');
	await new Promise((resolve) => setTimeout(resolve, 600));
	expect(state.revisions['tmgr.chatty']).toBe(1);
	host.dispose();
});

describe('plugin windows', () => {
	let generation = '';
	const gen = () => generation;

	const windowPlugin = (permissions: any[] = ['tasks:read']) =>
		({
			...pkg(
				'tmgr.win',
				`tmgr.commands.register('tmgr.win.hello', (args) => 'hello ' + args.name);`,
				permissions,
				{
					commands: [{ id: 'tmgr.win.hello', title: 'Hello' }],
					views: [{ id: 'board', title: 'Board', ui: 'ui/board.html' }],
				},
			),
			pages: { 'ui/board.html': '<h1>Board</h1>' },
		} as PluginPackage);

	it('opens a view in its own window with the page from the package', async () => {
		const opened: unknown[] = [];
		const { host } = setup(
			[windowPlugin()],
			{},
			{ open: async (...args: unknown[]) => void opened.push(args) },
		);
		await host.load();
		await host.activate(LOCAL);
		await host.openView('tmgr.win', 'board');
		expect(opened).toEqual([
			['tmgr.win/board', '<h1>Board</h1>', 'Board', expect.any(String)],
		]);
		host.dispose();
	});

	it('answers window calls through the plugin broker, with the same permissions', async () => {
		const { host } = setup([windowPlugin(['tasks:read'])], {
			listTasks: async () => ({ items: [{ id: 1 }], total: 1 }),
		});
		await host.load();
		await host.activate(LOCAL);
		generation = host.generationOf('tmgr.win')!;
		expect(await host.windowCall('tmgr.win', gen(), 'tasks.list', {})).toEqual({
			items: [{ id: 1 }],
			total: 1,
		});
		await expect(
			host.windowCall('tmgr.win', gen(), 'tasks.update', {
				id: 1,
				patch: { title: 'x' },
			}),
		).rejects.toMatchObject({
			code: 'PERMISSION_DENIED',
		});
		expect(
			await host.windowCall('tmgr.win', gen(), 'commands.run', {
				id: 'tmgr.win.hello',
				args: { name: 'Ann' },
			}),
		).toBe('hello Ann');
		await expect(
			host.windowCall('tmgr.win', gen(), 'commands.run', { id: 'other.cmd' }),
		).rejects.toMatchObject({
			code: 'NOT_DECLARED',
		});
		await expect(
			host.windowCall('tmgr.win', gen(), 'register', {
				kind: 'command',
				id: 'tmgr.win.hello',
			}),
		).rejects.toMatchObject({
			code: 'UNKNOWN_METHOD',
		});
		await host.setEnabled('tmgr.win', false);
		await expect(
			host.windowCall('tmgr.win', gen(), 'tasks.list', {}),
		).rejects.toMatchObject({ code: 'NOT_RUNNING' });
		host.dispose();
	});

	it('closes the windows of a plugin that stops', async () => {
		const closed: string[] = [];
		const { host } = setup(
			[windowPlugin()],
			{},
			{ close: async (pluginId: string) => void closed.push(pluginId) },
		);
		await host.load();
		await host.activate(LOCAL);
		await host.setEnabled('tmgr.win', false);
		expect(closed).toContain('tmgr.win');
		host.dispose();
	});
});

describe('plugin runs', () => {
	const runPlugin = pkg(
		'tmgr.run',
		`tmgr.commands.register('tmgr.run.go', () => 'ok');
		 tmgr.commands.register('tmgr.run.spin', () => { while (true) {} });`,
		['tasks:read'],
		{
			commands: [
				{ id: 'tmgr.run.go', title: 'Go' },
				{ id: 'tmgr.run.spin', title: 'Spin' },
			],
			views: [{ id: 'board', title: 'Board', ui: 'ui/board.html' }],
		},
	);
	(runPlugin as PluginPackage).pages = { 'ui/board.html': '<h1>B</h1>' };

	it('refuses window calls from an earlier run of the plugin', async () => {
		const { host } = setup([runPlugin]);
		await host.load();
		await host.activate(LOCAL);
		const first = host.generationOf('tmgr.run')!;
		await host.restart('tmgr.run');
		const second = host.generationOf('tmgr.run')!;
		expect(second).not.toBe(first);
		await expect(
			host.windowCall('tmgr.run', first, 'commands.run', { id: 'tmgr.run.go' }),
		).rejects.toMatchObject({
			code: 'NOT_RUNNING',
		});
		expect(
			await host.windowCall('tmgr.run', second, 'commands.run', {
				id: 'tmgr.run.go',
			}),
		).toBe('ok');
		host.dispose();
	});

	it('closes a window that finished opening after its run ended', async () => {
		const closed: string[] = [];
		let finishOpen: () => void = () => undefined;
		const { host } = setup(
			[runPlugin],
			{},
			{
				open: () => new Promise<void>((resolve) => (finishOpen = resolve)),
				close: async (pluginId: string) => void closed.push(pluginId),
			},
		);
		await host.load();
		await host.activate(LOCAL);
		const opening = host.openView('tmgr.run', 'board');
		await host.restart('tmgr.run');
		closed.length = 0;
		finishOpen();
		await opening;
		expect(closed).toEqual(['tmgr.run']);
		host.dispose();
	});

	it('restarts a plugin whose call timed out, so its unfinished work cannot write later', async () => {
		const { host, state } = setup([runPlugin]);
		await host.load();
		await host.activate(LOCAL);
		const before = host.generationOf('tmgr.run');
		await host.runCommand('tmgr.run', 'tmgr.run.spin').catch(() => undefined);
		await new Promise((resolve) => setTimeout(resolve, 100));
		expect(state.plugins['tmgr.run'].status).toBe('running');
		expect(host.generationOf('tmgr.run')).not.toBe(before);
		host.dispose();
	});
});

it('never runs a blocked plugin and stops one the blocklist catches while it runs', async () => {
	const blocked: Record<string, string> = { 'tmgr.bad': 'steals data' };
	const { host, state, notices } = setup(
		[pkg('tmgr.bad', ''), pkg('tmgr.good', '')],
		{},
		{},
		blocked,
	);
	await host.load();
	await host.activate(LOCAL);
	expect(state.plugins['tmgr.bad'].status).toBe('blocked');
	expect(state.plugins['tmgr.bad'].error).toBe('steals data');
	expect(state.plugins['tmgr.good'].status).toBe('running');

	await host.setEnabled('tmgr.bad', true);
	expect(state.plugins['tmgr.bad'].status).toBe('blocked');

	blocked['tmgr.good'] = 'compromised release';
	host.applyBlocklist();
	expect(state.plugins['tmgr.good'].status).toBe('blocked');
	expect(notices.join()).toContain('compromised release');

	delete blocked['tmgr.good'];
	host.applyBlocklist();
	await flush();
	expect(state.plugins['tmgr.good'].status).toBe('running');
	host.dispose();
});
