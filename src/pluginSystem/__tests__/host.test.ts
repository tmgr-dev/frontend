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
	extra: Partial<Parameters<typeof createPluginHost>[0]> = {},
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
		...extra,
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
		5: [
			{
				pluginId: 'tmgr.bar',
				badgeId: 'over',
				text: '5!',
				color: 'red',
				tooltip: null,
				priority: 0,
			},
		],
	});
	await host.setEnabled('tmgr.bar', false);
	expect(state.statusBar).toEqual({});
	host.dispose();
});

it('passes command args through host.runCommand, the same path pages and task sections use', async () => {
	const { host } = setup([
		pkg('tmgr.echo', `tmgr.commands.register('tmgr.echo.go', (args) => args);`, [], {
			commands: [{ id: 'tmgr.echo.go', title: 'Echo' }],
		}),
	]);
	await host.load();
	await host.activate(LOCAL);
	expect(
		await host.runCommand('tmgr.echo', 'tmgr.echo.go', { force: true, n: 3 }),
	).toEqual({ force: true, n: 3 });
	host.dispose();
});

it('refuses runCommand for a command not declared in the manifest or not registered by the plugin', async () => {
	const { host } = setup([
		pkg('tmgr.echo', `tmgr.commands.register('tmgr.echo.go', (args) => args);`, [], {
			commands: [
				{ id: 'tmgr.echo.go', title: 'Echo' },
				{ id: 'tmgr.echo.ghost', title: 'Ghost' },
			],
		}),
	]);
	await host.load();
	await host.activate(LOCAL);
	// declared in the manifest, but the plugin never registered a handler for it
	await expect(
		host.runCommand('tmgr.echo', 'tmgr.echo.ghost'),
	).rejects.toMatchObject({ code: 'NOT_DECLARED' });
	// not declared anywhere
	await expect(
		host.runCommand('tmgr.echo', 'not.a.command'),
	).rejects.toMatchObject({ code: 'NOT_DECLARED' });
	expect(
		await host.runCommand('tmgr.echo', 'tmgr.echo.go', { ok: true }),
	).toEqual({ ok: true });
	host.dispose();
});

it('accepts several badges per task, caps at 5 per plugin in total across its providers, and sorts by priority', async () => {
	const { host } = setup([
		pkg(
			'tmgr.multi',
			`tmgr.ui.provideBadges('m', () => ({
				5: [
					{ text: 'low', priority: 0 },
					{ text: 'high', priority: 5, key: 'Not Valid!' },
					{ text: 'mid', priority: 2, key: 'over-budget' },
					{ text: '4', priority: 1 },
					{ text: '5', priority: 1 },
					{ text: '6', priority: 1 },
				],
			}));`,
			['tasks:read'],
			{ boardCardBadges: [{ id: 'm' }] },
		),
	]);
	await host.load();
	await host.activate(LOCAL);
	await flush();
	const result = await host.badges([{ id: 5 }]);
	expect(result[5].map((b) => b.text)).toEqual(['high', 'mid', '4', '5', 'low']);
	expect(result[5][0]).toMatchObject({ text: 'high', priority: 5, key: undefined });
	expect(result[5][1]).toMatchObject({
		text: 'mid',
		priority: 2,
		key: 'over-budget',
	});
	host.dispose();
});

it('caps badges at 5 per plugin even when it registers several providers, and clamps priority', async () => {
	const { host } = setup([
		pkg(
			'tmgr.two',
			`tmgr.ui.provideBadges('a', () => ({ 5: [
				{ text: 'a1', priority: 999 }, { text: 'a2' }, { text: 'a3' }
			] }));
			 tmgr.ui.provideBadges('b', () => ({ 5: [
				{ text: 'b1', priority: -999 }, { text: 'b2' }, { text: 'b3' }
			] }));`,
			['tasks:read'],
			{ boardCardBadges: [{ id: 'a' }, { id: 'b' }] },
		),
	]);
	await host.load();
	await host.activate(LOCAL);
	await flush();
	const result = await host.badges([{ id: 5 }]);
	expect(result[5]).toHaveLength(5);
	expect(result[5].every((b) => b.pluginId === 'tmgr.two')).toBe(true);
	expect(result[5].find((b) => b.text === 'a1')).toMatchObject({
		badgeId: 'a',
		priority: 100,
	});
	expect(result[5].find((b) => b.text === 'b1')).toMatchObject({
		badgeId: 'b',
		priority: -100,
	});
	host.dispose();
});

describe('openLink', () => {
	const linkPackage = (
		permissions: string[],
		allowedDomains: string[],
	): PluginPackage => ({
		manifest: parseManifest({
			id: 'tmgr.link',
			name: 'Link',
			version: '1.0.0',
			engines: { tmgr: '^1.0' },
			permissions,
			links: { allowedDomains },
		}),
		code: '',
		source: 'builtin',
	});

	it('opens only an https link on a declared domain, from a plugin with links:open', async () => {
		const opened: string[] = [];
		const { host } = setup(
			[linkPackage(['links:open'], ['a.example.com'])],
			{},
			{},
			{},
			{ openExternal: (url: string) => opened.push(url) },
		);
		await host.load();
		await host.activate(LOCAL);
		expect(host.openLink('tmgr.link', 'https://a.example.com/x')).toBe(true);
		expect(host.openLink('tmgr.link', 'http://a.example.com/x')).toBe(false);
		expect(host.openLink('tmgr.link', 'https://evil.example.com/x')).toBe(false);
		expect(host.openLink('nope', 'https://a.example.com/x')).toBe(false);
		expect(opened).toEqual(['https://a.example.com/x']);
		host.dispose();
	});

	it('refuses links until the member allows the plugin on this computer', async () => {
		const opened: string[] = [];
		const { host } = setup(
			[linkPackage(['links:open'], ['a.example.com'])],
			{},
			{},
			{},
			{
				openExternal: (url: string) => opened.push(url),
				machineAllowed: () => false,
			},
		);
		await host.load();
		await host.activate(LOCAL);
		expect(host.openLink('tmgr.link', 'https://a.example.com/x')).toBe(false);
		expect(opened).toEqual([]);
		host.dispose();
	});

	it('refuses a link from a plugin without links:open, even to a domain it once had', async () => {
		const opened: string[] = [];
		const { host } = setup([linkPackage([], [])], {}, {}, {}, {
			openExternal: (url: string) => opened.push(url),
		});
		await host.load();
		await host.activate(LOCAL);
		expect(host.openLink('tmgr.link', 'https://a.example.com/x')).toBe(false);
		expect(opened).toEqual([]);
		host.dispose();
	});
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

it('renders a link node only for a plugin allowed to open that domain', async () => {
	const page = (id: string, allowedDomains: string[], permissions: string[]) => ({
		manifest: parseManifest({
			id,
			name: id,
			version: '1.0.0',
			engines: { tmgr: '^1.0' },
			permissions,
			links: { allowedDomains },
			contributes: { views: [{ id: 'report', title: 'Report' }] },
		}),
		code: `tmgr.ui.providePage('report', () => ({ type: 'link', url: 'https://a.example.com/x', text: 'Open' }));`,
		source: 'builtin' as const,
	});
	const { host } = setup([
		page('tmgr.allowed', ['a.example.com'], ['links:open']),
		page('tmgr.blocked', [], []),
	]);
	await host.load();
	await host.activate(LOCAL);
	await flush();
	expect(await host.renderPage('tmgr.allowed', 'report')).toEqual({
		type: 'link',
		url: 'https://a.example.com/x',
		text: 'Open',
		host: 'a.example.com',
	});
	expect(await host.renderPage('tmgr.blocked', 'report')).toEqual({
		type: 'text',
		text: 'Open',
		tone: 'default',
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

it('strips relationTypeWithTask from event task snapshots unless relations:read is granted', async () => {
	const seen: unknown[] = [];
	const { host, events } = setup(
		[
			pkg(
				'tmgr.norel',
				`tmgr.events.on('task.updated', (e) => tmgr.storage.set('e', e));`,
				['tasks:read'],
			),
			pkg(
				'tmgr.withrel',
				`tmgr.events.on('task.updated', (e) => tmgr.storage.set('e', e));`,
				['tasks:read', 'relations:read'],
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
		type: 'task.updated',
		workspaceId: LOCAL.id,
		taskId: 1,
		task: { title: 'Secret', relationTypeWithTask: [{ id: 9 }] },
	} as DomainEvent);
	await flush();
	expect(seen).toEqual(
		expect.arrayContaining([
			{ type: 'task.updated', workspaceId: LOCAL.id, taskId: 1, task: { title: 'Secret', key: null } },
			{
				type: 'task.updated',
				workspaceId: LOCAL.id,
				taskId: 1,
				task: { title: 'Secret', relationTypeWithTask: [{ id: 9 }], key: null },
			},
		]),
	);
	host.dispose();
});

it('drops actor-relative reacted/users from the broadcast comment.reactionChanged payload', async () => {
	const seen: unknown[] = [];
	const { host, events } = setup(
		[
			pkg(
				'tmgr.reactions',
				`tmgr.events.on('comment.reactionChanged', (e) => tmgr.storage.set('e', e));`,
				['comments:read'],
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
		type: 'comment.reactionChanged',
		workspaceId: LOCAL.id,
		commentId: 5,
		reactions: [{ emoji: '👍', count: 2, reacted: true, users: [{ id: 7, name: 'Yurij' }] }],
	} as DomainEvent);
	await flush();
	expect(seen).toEqual([
		{
			type: 'comment.reactionChanged',
			workspaceId: LOCAL.id,
			commentId: 5,
			reactions: [{ emoji: '👍', count: 2 }],
		},
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

it('in a cloud workspace runs only what is turned on there, and reaches this computer only with consent', async () => {
	const CLOUD = { id: 56, code: 'team', name: 'Team', kind: 'cloud' as const };
	const fetches: string[] = [];
	const allowed = new Set<string>();
	const net: PluginPackage = {
		manifest: parseManifest({
			id: 'tmgr.net',
			name: 'Net',
			version: '1.0.0',
			engines: { tmgr: '^1.0' },
			network: { allowedOrigins: ['http://localhost:11434'] },
			contributes: { commands: [{ id: 'tmgr.net.go', title: 'Go' }] },
		}),
		code: `tmgr.commands.register('tmgr.net.go', async () => {
			try { await tmgr.net.fetch('http://localhost:11434/x'); return 'fetched'; }
			catch (error) { return error.message; }
		});`,
		source: 'builtin',
	};
	const { host, state, leave } = setup([net, pkg('tmgr.idle', '')], {}, {}, {}, {
		enabled: {
			get: (pluginId, workspaceId) =>
				workspaceId === CLOUD.id ? pluginId === 'tmgr.net' : undefined,
			set: () => undefined,
		},
		fetch: async (request) => {
			fetches.push(request.url);
			return { status: 200, headers: [], body: 'ok' };
		},
		machineAllowed: (pluginId, workspace) =>
			workspace.kind === 'local' || allowed.has(pluginId),
	});
	await host.load();
	leave();
	await host.activate(CLOUD);
	expect(state.plugins['tmgr.net'].status).toBe('running');
	expect(state.plugins['tmgr.idle'].status).toBe('stopped');
	expect(await host.runCommand('tmgr.net', 'tmgr.net.go')).toMatch(
		/not available/i,
	);
	expect(fetches).toEqual([]);

	allowed.add('tmgr.net');
	await host.restart('tmgr.net');
	expect(await host.runCommand('tmgr.net', 'tmgr.net.go')).toBe('fetched');
	expect(fetches).toEqual(['http://localhost:11434/x']);
	host.dispose();
});

describe('alarms', () => {
	const alarmPlugin = pkg(
		'tmgr.alarm',
		`tmgr.commands.register('tmgr.alarm.create', (a) => tmgr.alarms.create(a.name, a.opts).catch((e) => ({ error: e.name })));
		 tmgr.commands.register('tmgr.alarm.list', () => tmgr.alarms.list());
		 tmgr.commands.register('tmgr.alarm.clear', (a) => tmgr.alarms.clear(a.name));
		 tmgr.events.on('alarm', (e) => tmgr.storage.set('fired', e));`,
		['alarms'],
		{
			commands: [
				{ id: 'tmgr.alarm.create', title: 'Create' },
				{ id: 'tmgr.alarm.list', title: 'List' },
				{ id: 'tmgr.alarm.clear', title: 'Clear' },
			],
		},
	);

	const stored = () => {
		const rows: unknown[] = [];
		return {
			rows,
			api: { storageSet: async (key: string, json: string) => void rows.push([key, JSON.parse(json)]) },
		};
	};

	it('validates name, minimum delay/period, and mixing when with delay/period', async () => {
		const { host } = setup([alarmPlugin]);
		await host.load();
		await host.activate(LOCAL);
		const create = (name: string, opts: unknown) =>
			host.runCommand('tmgr.alarm', 'tmgr.alarm.create', { name, opts });
		expect(await create('ok', { delayMinutes: 1 })).toMatchObject({ name: 'ok' });
		expect(await create('too short', { delayMinutes: 0.5 })).toEqual({
			error: 'INVALID_PARAMS',
		});
		expect(await create('bad name!', { delayMinutes: 1 })).toEqual({
			error: 'INVALID_PARAMS',
		});
		expect(await create('a'.repeat(61), { delayMinutes: 1 })).toEqual({
			error: 'INVALID_PARAMS',
		});
		expect(await create('mixed', { delayMinutes: 1, when: '2999-01-01T00:00:00Z' })).toEqual({
			error: 'INVALID_PARAMS',
		});
		expect(await create('past', { when: '2000-01-01T00:00:00Z' })).toEqual({
			error: 'INVALID_PARAMS',
		});
		expect(await create('far', { when: '2999-01-01T00:00:00Z' })).toEqual({
			error: 'INVALID_PARAMS',
		});
		expect(await create('none', {})).toEqual({ error: 'INVALID_PARAMS' });
		host.dispose();
	});

	it('allows at most 10 alarms per plugin per workspace, but an update does not count', async () => {
		const { host } = setup([alarmPlugin]);
		await host.load();
		await host.activate(LOCAL);
		const create = (name: string) =>
			host.runCommand('tmgr.alarm', 'tmgr.alarm.create', {
				name,
				opts: { delayMinutes: 5 },
			});
		for (let i = 0; i < 10; i++) expect(await create(`a${i}`)).toMatchObject({ name: `a${i}` });
		expect(await create('a10')).toEqual({ error: 'INVALID_PARAMS' });
		expect(await create('a0')).toMatchObject({ name: 'a0' });
		host.dispose();
	});

	it('needs the alarms permission', async () => {
		const { host } = setup([
			pkg(
				'tmgr.noalarm',
				`tmgr.commands.register('tmgr.noalarm.create', (a) => tmgr.alarms.create(a.name, a.opts).catch((e) => ({ error: e.name })));`,
				[],
				{ commands: [{ id: 'tmgr.noalarm.create', title: 'Create' }] },
			),
		]);
		await host.load();
		await host.activate(LOCAL);
		expect(
			await host.runCommand('tmgr.noalarm', 'tmgr.noalarm.create', {
				name: 'x',
				opts: { delayMinutes: 1 },
			}),
		).toEqual({ error: 'PERMISSION_DENIED' });
		host.dispose();
	});

	it('fires a due alarm through dispatch, and reschedules a periodic one from now', async () => {
		let clock = 1_000_000;
		const { rows, api } = stored();
		const { host } = setup([alarmPlugin], api, {}, {}, { now: () => clock });
		await host.load();
		await host.activate(LOCAL);
		await host.runCommand('tmgr.alarm', 'tmgr.alarm.create', {
			name: 'p',
			opts: { periodMinutes: 5 },
		});
		clock += 5 * 60_000;
		host.tick();
		await flush();
		expect(rows).toHaveLength(1);
		expect(rows[0]).toEqual(['fired', { name: 'p', scheduledAt: new Date(1_000_000 + 5 * 60_000).toISOString() }]);
		const list = (await host.runCommand('tmgr.alarm', 'tmgr.alarm.list', {})) as { name: string; scheduledAt: string }[];
		expect(list).toEqual([{ name: 'p', scheduledAt: new Date(clock + 5 * 60_000).toISOString() }]);
		host.dispose();
	});

	it('removes a one-shot alarm once it fires', async () => {
		let clock = 0;
		const { rows, api } = stored();
		const { host } = setup([alarmPlugin], api, {}, {}, { now: () => clock });
		await host.load();
		await host.activate(LOCAL);
		await host.runCommand('tmgr.alarm', 'tmgr.alarm.create', { name: 'once', opts: { delayMinutes: 1 } });
		clock += 60_000;
		host.tick();
		await flush();
		expect(rows).toHaveLength(1);
		expect(await host.runCommand('tmgr.alarm', 'tmgr.alarm.list', {})).toEqual([]);
		host.dispose();
	});

	it('coalesces missed firings after a long gap into a single event', async () => {
		let clock = 0;
		const { rows, api } = stored();
		const { host } = setup([alarmPlugin], api, {}, {}, { now: () => clock });
		await host.load();
		await host.activate(LOCAL);
		await host.runCommand('tmgr.alarm', 'tmgr.alarm.create', {
			name: 'p',
			opts: { periodMinutes: 5 },
		});
		clock += 60 * 60_000;
		host.tick();
		await flush();
		expect(rows).toHaveLength(1);
		host.dispose();
	});

	it('reloads persisted alarms on start, and nothing fires after stop or in safe mode', async () => {
		let clock = 0;
		const store: Record<string, unknown> = {};
		const alarms = {
			get: (key: string) => store[key] as any,
			set: (key: string, defs: unknown) => void (store[key] = defs),
		};
		const { rows, api } = stored();
		const { host, state } = setup([alarmPlugin], api, {}, {}, { now: () => clock, alarms });
		await host.load();
		await host.activate(LOCAL);
		await host.runCommand('tmgr.alarm', 'tmgr.alarm.create', { name: 'p', opts: { delayMinutes: 1 } });
		clock += 60_000;

		await host.setEnabled('tmgr.alarm', false);
		host.tick();
		await flush();
		expect(rows).toHaveLength(0);

		state.safeMode = true;
		await host.activate(LOCAL);
		host.tick();
		await flush();
		expect(rows).toHaveLength(0);
		state.safeMode = false;

		const second = setup([alarmPlugin], api, {}, {}, { now: () => clock, alarms }).host;
		await second.load();
		await second.activate(LOCAL);
		second.tick();
		await flush();
		expect(rows).toHaveLength(1);
		second.dispose();
		host.dispose();
	});
});

describe('start events', () => {
	const CLOUD = { id: 56, code: 'team', name: 'Team', kind: 'cloud' as const };
	const startPlugin = pkg(
		'tmgr.start',
		`tmgr.events.on('app.started', () => tmgr.storage.set('started', 1));
		 tmgr.events.on('workspace.switched', (e) => tmgr.storage.set('switched', e));`,
	);

	it('delivers app.started once per session, even across restarts', async () => {
		const rows: [string, unknown][] = [];
		const { host } = setup([startPlugin], {
			storageSet: async (key: string, json: string) => void rows.push([key, JSON.parse(json)]),
		});
		await host.load();
		await host.activate(LOCAL);
		await flush();
		expect(rows.filter(([k]) => k === 'started')).toHaveLength(1);
		await host.setEnabled('tmgr.start', false);
		await host.setEnabled('tmgr.start', true);
		await flush();
		expect(rows.filter(([k]) => k === 'started')).toHaveLength(1);
		host.dispose();
	});

	it('delivers workspace.switched with from/to only when the active workspace actually changes', async () => {
		const rows: [string, unknown][] = [];
		const { host, leave } = setup(
			[startPlugin],
			{ storageSet: async (key: string, json: string) => void rows.push([key, JSON.parse(json)]) },
			{},
			{},
			{ enabled: { get: () => true, set: () => undefined } },
		);
		await host.load();
		await host.activate(LOCAL);
		await flush();
		expect(rows.filter(([k]) => k === 'switched')).toHaveLength(0);
		leave();
		await host.activate(CLOUD);
		await flush();
		expect(rows.filter(([k]) => k === 'switched')).toEqual([
			['switched', { from: LOCAL.id, to: CLOUD.id }],
		]);
		await host.activate(CLOUD);
		await flush();
		expect(rows.filter(([k]) => k === 'switched')).toHaveLength(1);
		host.dispose();
	});
});

describe('notifications', () => {
	const notifyPlugin = pkg(
		'tmgr.notif',
		`tmgr.commands.register('tmgr.notif.act', (args) => tmgr.storage.set('acted', args));
		 tmgr.commands.register('tmgr.notif.send', (args) => tmgr.ui.notify('hi', args).catch((e) => ({ error: e.name })));
		 tmgr.commands.register('tmgr.notif.dnd', () => tmgr.ui.dnd());`,
		['notifications'],
		{
			commands: [
				{ id: 'tmgr.notif.act', title: 'Act' },
				{ id: 'tmgr.notif.send', title: 'Send' },
				{ id: 'tmgr.notif.dnd', title: 'DND' },
			],
		},
	);

	const setupNotify = (extra: Partial<Parameters<typeof createPluginHost>[0]> = {}) => {
		const notifications: any[] = [];
		const stored: unknown[] = [];
		const rest = setup(
			[notifyPlugin],
			{ storageSet: async (key: string, json: string) => void stored.push([key, JSON.parse(json)]) },
			{},
			{},
			{
				notifyPlugin: (_pluginId, _name, payload) => notifications.push(payload),
				...extra,
			},
		);
		return { ...rest, notifications, stored };
	};

	it('validates title, message, args size, and requires a declared command', async () => {
		const { host } = setupNotify();
		await host.load();
		await host.activate(LOCAL);
		const send = (args: unknown) => host.runCommand('tmgr.notif', 'tmgr.notif.send', args);
		expect(await send({ message: 'x'.repeat(301) })).toEqual({ error: 'INVALID_PARAMS' });
		expect(await send({ title: 'x'.repeat(81) })).toEqual({ error: 'INVALID_PARAMS' });
		expect(await send({ args: { big: 'x'.repeat(5000) } })).toEqual({ error: 'INVALID_PARAMS' });
		expect(await send({ command: 'not.declared' })).toEqual({ error: 'NOT_DECLARED' });
		expect(await send({ command: 'tmgr.notif.act' })).toBeNull();
		host.dispose();
	});

	it('limits a plugin to 5 notifications per minute', async () => {
		const { host } = setupNotify();
		await host.load();
		await host.activate(LOCAL);
		const results = [];
		for (let i = 0; i < 6; i++)
			results.push(await host.runCommand('tmgr.notif', 'tmgr.notif.send', {}));
		expect(results.slice(0, 5)).toEqual([null, null, null, null, null]);
		expect(results[5]).toEqual({ error: 'RATE_LIMITED' });
		host.dispose();
	});

	it('a notification click runs the declared command only while the plugin still runs in that workspace', async () => {
		const { host, notifications, stored, leave } = setupNotify();
		await host.load();
		await host.activate(LOCAL);
		await host.runCommand('tmgr.notif', 'tmgr.notif.send', {
			command: 'tmgr.notif.act',
			args: { x: 1 },
		});
		const token = notifications[0].token;
		expect(token).toEqual(expect.any(String));
		expect(await host.resolveNotificationClick(token)).toEqual({ type: 'command' });
		await flush();
		expect(stored).toEqual([['acted', { x: 1 }]]);

		await host.runCommand('tmgr.notif', 'tmgr.notif.send', { command: 'tmgr.notif.act' });
		const secondToken = notifications[1].token;
		leave();
		expect(await host.resolveNotificationClick(secondToken)).toBeNull();
		host.dispose();
	});

	it('drops pending click tokens when the plugin stops', async () => {
		const { host, notifications } = setupNotify();
		await host.load();
		await host.activate(LOCAL);
		await host.runCommand('tmgr.notif', 'tmgr.notif.send', { taskId: 5 });
		const token = notifications[0].token;
		await host.setEnabled('tmgr.notif', false);
		expect(await host.resolveNotificationClick(token)).toBeNull();
		host.dispose();
	});

	it('suppresses notifications while do-not-disturb is active, and exposes it read-only', async () => {
		const { host, notifications } = setupNotify({ dnd: () => ({ active: true, until: '2999-01-01T00:00:00.000Z' }) });
		await host.load();
		await host.activate(LOCAL);
		await host.runCommand('tmgr.notif', 'tmgr.notif.send', {});
		expect(notifications).toEqual([]);
		expect(await host.runCommand('tmgr.notif', 'tmgr.notif.dnd', {})).toEqual({
			active: true,
			until: '2999-01-01T00:00:00.000Z',
		});
		host.dispose();
	});
});
