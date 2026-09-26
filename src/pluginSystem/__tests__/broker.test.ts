import {
	createBroker,
	PluginError,
	type BrokerDeps,
	type DataApi,
} from '../broker';
import { parseManifest, type Permission } from '../manifest';

const manifest = (permissions: Permission[]) =>
	parseManifest({
		id: 'tmgr.test',
		name: 'Test',
		version: '1.0.0',
		engines: { tmgr: '^1.0' },
		permissions,
		contributes: {
			boardCardBadges: [{ id: 'overrun' }],
			statusBarItems: [{ id: 'total' }],
			commands: [{ id: 'tmgr.test.go', title: 'Go' }],
			views: [{ id: 'report', title: 'Report' }],
			taskPanelSections: [{ id: 'summary', title: 'Summary' }],
			settings: {
				type: 'object',
				properties: { warnAt: { type: 'number', default: 0.8 } },
			},
		},
	});

const fakeApi = (): DataApi & { calls: unknown[][] } => {
	const calls: unknown[][] = [];
	const record =
		(name: string) =>
		async (...args: unknown[]) => {
			calls.push([name, ...args]);
			return { name };
		};
	return {
		calls,
		listTasks: record('listTasks'),
		getTask: record('getTask'),
		createTask: record('createTask'),
		updateTask: record('updateTask'),
		listStatuses: record('listStatuses'),
		listCategories: record('listCategories'),
		startTimer: record('startTimer'),
		stopTimer: record('stopTimer'),
		listComments: record('listComments'),
		addComment: record('addComment'),
		storageGet: record('storageGet'),
		storageSet: record('storageSet'),
		storageDelete: record('storageDelete'),
		storageKeys: record('storageKeys'),
	};
};

const setup = (
	permissions: Permission[],
	overrides: Partial<BrokerDeps> = {},
) => {
	const api = fakeApi();
	const registered: unknown[] = [];
	const notified: string[] = [];
	const statusBar: unknown[] = [];
	let clock = 0;
	let current: number | null = -7;
	const broker = createBroker({
		manifest: manifest(permissions),
		workspace: { id: -7, code: 'local-notes', name: 'Notes', kind: 'local' },
		currentWorkspaceId: () => current,
		api,
		settings: () => ({ warnAt: 0.9 }),
		notify: (message) => notified.push(message),
		setStatusBarItem: (id, item) => statusBar.push([id, item]),
		refresh: () => undefined,
		register: (kind, id) => registered.push([kind, id]),
		log: () => undefined,
		now: () => clock,
		...overrides,
	});
	return {
		broker,
		api,
		registered,
		notified,
		statusBar,
		tick: (ms: number) => (clock += ms),
		switchTo: (id: number | null) => (current = id),
	};
};

const code = async (promise: Promise<unknown>) => {
	try {
		await promise;
		return 'ok';
	} catch (error) {
		return error instanceof PluginError ? error.code : `unexpected ${error}`;
	}
};

it('reads and writes only what the manifest grants', async () => {
	const { broker, api } = setup(['tasks:read']);
	expect(await code(broker.call('tasks.list', { statusId: 3 }))).toBe('ok');
	expect(
		await code(broker.call('tasks.update', { id: 1, patch: { title: 'x' } })),
	).toBe('PERMISSION_DENIED');
	expect(await code(broker.call('time.start', { taskId: 1 }))).toBe(
		'PERMISSION_DENIED',
	);
	expect(
		await code(broker.call('comments.add', { taskId: 1, text: 'hi' })),
	).toBe('PERMISSION_DENIED');
	expect(await code(broker.call('ui.notify', { message: 'hi' }))).toBe(
		'PERMISSION_DENIED',
	);
	expect(api.calls).toEqual([
		[
			'listTasks',
			{ statusId: 3, categoryId: null, search: null, page: 1, perPage: 50 },
		],
	]);
});

it('passes only allowlisted, well-typed task fields', async () => {
	const { broker, api } = setup(['tasks:write']);
	await broker.call('tasks.update', {
		id: 4,
		patch: {
			title: 'New',
			status_id: 2,
			user_id: 99,
			workspace_id: 1,
			approximately_time: 60,
		},
	});
	expect(api.calls).toEqual([
		['updateTask', 4, { title: 'New', status_id: 2, approximately_time: 60 }],
	]);
	expect(await code(broker.call('tasks.update', { id: 'x', patch: {} }))).toBe(
		'INVALID_PARAMS',
	);
	expect(
		await code(
			broker.call('tasks.update', { id: 4, patch: { priority: 'critical' } }),
		),
	).toBe('INVALID_PARAMS');
	expect(await code(broker.call('tasks.create', { title: '' }))).toBe(
		'INVALID_PARAMS',
	);
	expect(await code(broker.call('tasks.delete', { id: 4 }))).toBe(
		'UNKNOWN_METHOD',
	);
});

it('refuses every call once the app has left the plugin workspace', async () => {
	const { broker, switchTo } = setup(['tasks:read']);
	switchTo(56);
	expect(await code(broker.call('tasks.list', {}))).toBe('WORKSPACE_CHANGED');
});

it('limits writes per second', async () => {
	const { broker, tick } = setup(['tasks:write']);
	const results = [];
	for (let i = 0; i < 25; i++)
		results.push(
			await code(
				broker.call('tasks.update', { id: 1, patch: { title: `t${i}` } }),
			),
		);
	expect(results.filter((r) => r === 'ok')).toHaveLength(20);
	expect(results[results.length - 1]).toBe('RATE_LIMITED');
	tick(1000);
	expect(
		await code(
			broker.call('tasks.update', { id: 1, patch: { title: 'later' } }),
		),
	).toBe('ok');
});

it('registers only what the manifest declares, and events only with read access', async () => {
	const { broker, registered } = setup(['time:read', 'tasks:read']);
	expect(
		await code(
			broker.call('register', { kind: 'command', id: 'tmgr.test.go' }),
		),
	).toBe('ok');
	expect(
		await code(
			broker.call('register', { kind: 'command', id: 'tmgr.test.other' }),
		),
	).toBe('NOT_DECLARED');
	expect(
		await code(broker.call('register', { kind: 'badges', id: 'overrun' })),
	).toBe('ok');
	expect(
		await code(broker.call('register', { kind: 'page', id: 'report' })),
	).toBe('ok');
	expect(
		await code(broker.call('register', { kind: 'section', id: 'summary' })),
	).toBe('ok');
	expect(
		await code(broker.call('register', { kind: 'event', id: 'timer.stopped' })),
	).toBe('ok');
	expect(
		await code(
			broker.call('register', { kind: 'event', id: 'comment.created' }),
		),
	).toBe('PERMISSION_DENIED');
	expect(
		await code(broker.call('register', { kind: 'event', id: 'app.quit' })),
	).toBe('INVALID_PARAMS');
	expect(registered).toEqual([
		['command', 'tmgr.test.go'],
		['badges', 'overrun'],
		['page', 'report'],
		['section', 'summary'],
		['event', 'timer.stopped'],
	]);
});

it('ignores inherited names in task patches', async () => {
	const { broker, api } = setup(['tasks:write']);
	await broker.call('tasks.update', {
		id: 4,
		patch: JSON.parse(
			'{"constructor":{"workspace_id":5},"toString":1,"hasOwnProperty":false,"title":"ok"}',
		),
	});
	expect(api.calls).toEqual([['updateTask', 4, { title: 'ok' }]]);
});

it('needs tasks:read to receive task snapshots in badges or sections', async () => {
	const { broker } = setup([]);
	expect(
		await code(broker.call('register', { kind: 'badges', id: 'overrun' })),
	).toBe('PERMISSION_DENIED');
	expect(
		await code(broker.call('register', { kind: 'section', id: 'summary' })),
	).toBe('PERMISSION_DENIED');
	expect(
		await code(broker.call('register', { kind: 'page', id: 'report' })),
	).toBe('ok');
});

it('keeps plugin storage within its quota and hands out settings and the workspace', async () => {
	const { broker, api, statusBar } = setup([]);
	expect(await broker.call('settings.get', {})).toEqual({ warnAt: 0.9 });
	expect(await broker.call('workspace.current', {})).toEqual({
		id: -7,
		code: 'local-notes',
		name: 'Notes',
		kind: 'local',
	});
	await broker.call('storage.set', { key: 'seen', value: { a: 1 } });
	expect(api.calls).toEqual([['storageSet', 'seen', '{"a":1}']]);
	expect(
		await code(
			broker.call('storage.set', { key: 'big', value: 'x'.repeat(300_000) }),
		),
	).toBe('INVALID_PARAMS');
	expect(
		await code(
			broker.call('ui.setStatusBarItem', { id: 'total', text: '2h over' }),
		),
	).toBe('ok');
	expect(
		await code(broker.call('ui.setStatusBarItem', { id: 'other', text: 'x' })),
	).toBe('NOT_DECLARED');
	expect(statusBar).toEqual([
		['total', { text: '2h over', tooltip: null, command: null }],
	]);
});
