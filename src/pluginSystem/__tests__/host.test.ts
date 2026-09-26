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

const setup = (packages: PluginPackage[], api: Partial<DataApi> = {}) => {
	const state: PluginHostState = {
		workspace: null,
		safeMode: false,
		plugins: {},
		statusBar: {},
		revision: 0,
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
			[],
			{ statusBarItems: [{ id: 'total' }], boardCardBadges: [{ id: 'over' }] },
		),
	]);
	await host.load();
	await host.activate(LOCAL);
	await flush();
	expect(Object.values(state.statusBar)).toEqual([
		{
			pluginId: 'tmgr.bar',
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
	for (let i = 0; i < 3; i++)
		await host.runCommand('tmgr.spin', 'tmgr.spin.go').catch(() => undefined);
	expect(state.plugins['tmgr.spin'].status).toBe('crashed');
	expect(notices).toEqual([
		expect.stringMatching(/^tmgr.spin was turned off: It failed 3 times/),
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
