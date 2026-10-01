import {
	createBroker,
	PluginError,
	type BrokerDeps,
	type DataApi,
} from '../broker';
import { parseManifest, type Permission } from '../manifest';

const manifest = (permissions: Permission[], allowedOrigins: string[] = []) =>
	parseManifest({
		network: { allowedOrigins },
		id: 'tmgr.test',
		name: 'Test',
		version: '1.0.0',
		engines: { tmgr: '^1.5' },
		permissions,
		contributes: {
			boardCardBadges: [{ id: 'overrun' }],
			statusBarItems: [{ id: 'total' }],
			trayItems: [{ id: 'menu' }],
			commands: [{ id: 'tmgr.test.go', title: 'Go' }],
			views: [{ id: 'report', title: 'Report' }],
			taskPanelSections: [{ id: 'summary', title: 'Summary' }],
			settings: {
				type: 'object',
				properties: { warnAt: { type: 'number', default: 0.8 } },
			},
		},
	});

const fakeRoutine = { id: 0, title: '', description: null, scheduledDate: null, scheduledTime: null, createdAt: '', updatedAt: '' };
const fakeInstance = { id: 0, routineId: 0, date: '', time: null, status: 'PENDING' as const };

const fakeApi = (): DataApi & { calls: unknown[][] } => {
	const calls: unknown[][] = [];
	const record =
		(name: string) =>
		async (...args: unknown[]) => {
			calls.push([name, ...args]);
			return { name };
		};
	const recordAs =
		<T,>(name: string, value: T) =>
		async (...args: unknown[]) => {
			calls.push([name, ...args]);
			return value;
		};
	return {
		calls,
		listTasks: record('listTasks'),
		getTask: record('getTask'),
		createTask: record('createTask'),
		updateTask: record('updateTask'),
		listStatuses: record('listStatuses'),
		listCategories: record('listCategories'),
		createStatus: record('createStatus'),
		updateStatus: record('updateStatus'),
		reorderStatuses: record('reorderStatuses'),
		createCategory: record('createCategory'),
		updateCategory: record('updateCategory'),
		startTimer: record('startTimer'),
		stopTimer: record('stopTimer'),
		listComments: record('listComments'),
		addComment: record('addComment'),
		reactToComment: record('reactToComment'),
		listRelations: record('listRelations'),
		relateTask: record('relateTask'),
		unrelateTask: record('unrelateTask'),
		storageGet: record('storageGet'),
		storageSet: record('storageSet'),
		storageDelete: record('storageDelete'),
		storageKeys: record('storageKeys'),
		listAttachments: record('listAttachments'),
		readAttachment: record('readAttachment'),
		taskDataGet: record('taskDataGet'),
		taskDataSet: record('taskDataSet'),
		taskDataDelete: record('taskDataDelete'),
		taskDataGetMany: record('taskDataGetMany'),
		pagesSearch: record('pagesSearch'),
		pagesTree: record('pagesTree'),
		pagesGet: async (...args: unknown[]) => {
			calls.push(['pagesGet', ...args]);
			return {
				id: 5,
				sections: [
					{ id: 'mine', owner: 'plugin:tmgr.test', heading: 'Mine' },
					{ id: 'theirs', owner: 'plugin:tmgr.other', heading: null },
					{ id: 'shared', owner: 'agents', heading: null },
				],
			};
		},
		pagesCreate: record('pagesCreate'),
		pagesUpdate: record('pagesUpdate'),
		pagesAppend: record('pagesAppend'),
		pagesSetSection: record('pagesSetSection'),
		pageDataGet: record('pageDataGet'),
		pageDataSet: record('pageDataSet'),
		pageDataDelete: record('pageDataDelete'),
		pageDataGetMany: async () => ({ 5: '{"a":1}', 6: '2' }),
		listAgentWork: record('listAgentWork'),
		startAgentWork: record('startAgentWork'),
		updateAgentWork: record('updateAgentWork'),
		finishAgentWork: record('finishAgentWork'),
		listRoutines: recordAs('listRoutines', []),
		getRoutine: recordAs('getRoutine', fakeRoutine),
		listRoutineInstances: recordAs('listRoutineInstances', []),
		createRoutine: recordAs('createRoutine', fakeRoutine),
		updateRoutine: recordAs('updateRoutine', fakeRoutine),
		completeRoutine: recordAs('completeRoutine', fakeInstance),
		skipRoutine: recordAs('skipRoutine', fakeInstance),
		convertRoutine: record('convertRoutine'),
	};
};

const setup = (
	permissions: Permission[],
	overrides: Partial<BrokerDeps> = {},
	allowedOrigins: string[] = [],
) => {
	const api = fakeApi();
	const registered: unknown[] = [];
	const notified: string[] = [];
	const statusBar: unknown[] = [];
	let clock = 0;
	let current: number | null = -7;
	const broker = createBroker({
		manifest: manifest(permissions, allowedOrigins),
		workspace: { id: -7, code: 'local-notes', name: 'Notes', kind: 'local' },
		currentWorkspaceId: () => current,
		api,
		settings: () => ({ warnAt: 0.9 }),
		notify: (payload) => notified.push(payload.message),
		setStatusBarItem: (id, item) => statusBar.push([id, item]),
		refresh: () => undefined,
		setViewBadge: () => undefined,
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

it('gates reactions and relations behind their permissions, validated and rate-limited as writes', async () => {
	const { broker, api } = setup(['comments:read', 'relations:read']);
	expect(
		await code(broker.call('comments.react', { commentId: 1, emoji: '👍' })),
	).toBe('PERMISSION_DENIED');
	expect(
		await code(broker.call('tasks.relate', { taskId: 1, otherId: 2, type: 'blocks' })),
	).toBe('PERMISSION_DENIED');
	expect(
		await code(broker.call('tasks.unrelate', { taskId: 1, otherId: 2, type: 'blocks' })),
	).toBe('PERMISSION_DENIED');
	expect(await code(broker.call('tasks.relations', { taskId: 1 }))).toBe('ok');
	expect(api.calls).toEqual([['listRelations', 1]]);

	const { broker: writer, api: writerApi } = setup([
		'comments:write',
		'relations:write',
	]);
	expect(
		await code(writer.call('comments.react', { commentId: 1, emoji: '  ' })),
	).toBe('INVALID_PARAMS');
	expect(
		await code(writer.call('comments.react', { commentId: 1, emoji: 'x'.repeat(33) })),
	).toBe('INVALID_PARAMS');
	expect(
		await code(writer.call('tasks.relate', { taskId: 1, otherId: 2, type: 'nope' })),
	).toBe('INVALID_PARAMS');
	expect(await code(writer.call('comments.react', { commentId: 1, emoji: '👍' }))).toBe(
		'ok',
	);
	expect(
		await code(writer.call('tasks.relate', { taskId: 1, otherId: 2, type: 'blocks' })),
	).toBe('ok');
	expect(
		await code(writer.call('tasks.unrelate', { taskId: 1, otherId: 2, type: 'blocks' })),
	).toBe('ok');
	expect(writerApi.calls).toEqual([
		['reactToComment', 1, '👍'],
		['relateTask', 1, 2, 'blocks'],
		['unrelateTask', 1, 2, 'blocks'],
	]);

	const { broker: burstWriter } = setup(['comments:write']);
	const results: string[] = [];
	for (let i = 0; i < 21; i++) {
		results.push(await code(burstWriter.call('comments.react', { commentId: 1, emoji: '👍' })));
	}
	expect(results.filter((r) => r === 'ok')).toHaveLength(20);
	expect(results[20]).toBe('RATE_LIMITED');
});

it('strips relationTypeWithTask from every task-returning call unless relations:read is granted', async () => {
	const withRelation = { id: 1, title: 'x', relationTypeWithTask: [{ id: 9 }] };
	const api: DataApi = {
		...fakeApi(),
		listTasks: async () => ({ items: [withRelation], total: 1 }),
		getTask: async () => withRelation,
		createTask: async () => withRelation,
		updateTask: async () => withRelation,
		startTimer: async () => withRelation,
		stopTimer: async () => withRelation,
	};
	const { broker } = setup(['tasks:read', 'tasks:write', 'time:write'], { api });
	expect(await broker.call('tasks.list', {})).toEqual({ items: [{ id: 1, title: 'x' }], total: 1 });
	expect(await broker.call('tasks.get', { id: 1 })).toEqual({ id: 1, title: 'x' });
	expect(await broker.call('tasks.create', { title: 'x' })).toEqual({ id: 1, title: 'x' });
	expect(await broker.call('tasks.update', { id: 1, patch: { title: 'x' } })).toEqual({ id: 1, title: 'x' });
	expect(await broker.call('time.start', { taskId: 1 })).toEqual({ id: 1, title: 'x' });
	expect(await broker.call('time.stop', { taskId: 1 })).toEqual({ id: 1, title: 'x' });

	const { broker: withRelations } = setup(
		['tasks:read', 'relations:read'],
		{ api: { ...fakeApi(), getTask: async () => withRelation } },
	);
	expect(await withRelations.call('tasks.get', { id: 1 })).toEqual(withRelation);
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
	// Byte-accurate: 88,000 CJK characters are under 256K UTF-16 code units but over 256 KB in UTF-8.
	expect(
		await code(
			broker.call('storage.set', { key: 'cjk', value: '字'.repeat(88_000) }),
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

it('fetches only from the origins the manifest lists', async () => {
	const fetched: unknown[] = [];
	const { broker } = setup(
		[],
		{
			fetch: async (request) => {
				fetched.push(request);
				return { status: 200, headers: [], body: '{}' };
			},
		},
		['http://localhost:11434'],
	);
	expect(
		await broker.call('net.fetch', {
			url: 'http://localhost:11434/api/generate',
			method: 'POST',
			body: '{}',
		}),
	).toEqual({
		status: 200,
		headers: [],
		body: '{}',
	});
	expect(
		await code(broker.call('net.fetch', { url: 'http://localhost:9999/' })),
	).toBe('PERMISSION_DENIED');
	expect(
		await code(broker.call('net.fetch', { url: 'http://127.0.0.1:11434/' })),
	).toBe('PERMISSION_DENIED');
	expect(await code(broker.call('net.fetch', { url: 'not a url' }))).toBe(
		'INVALID_PARAMS',
	);
	expect(fetched).toEqual([
		{
			url: 'http://localhost:11434/api/generate',
			method: 'POST',
			headers: [],
			body: '{}',
		},
	]);
	const { broker: offline } = setup([]);
	expect(
		await code(offline.call('net.fetch', { url: 'http://localhost:11434/' })),
	).toBe('PERMISSION_DENIED');
});

it('accepts expired_at as a nullable ISO date-time task field, normalised to UTC', async () => {
	const { broker, api } = setup(['tasks:write']);
	await broker.call('tasks.update', { id: 4, patch: { expired_at: '2026-10-01T12:30:00Z' } });
	await broker.call('tasks.update', { id: 4, patch: { expired_at: '2026-10-01T14:30:00+02:00' } });
	await broker.call('tasks.update', { id: 4, patch: { expired_at: null } });
	expect(api.calls).toEqual([
		['updateTask', 4, { expired_at: '2026-10-01T12:30:00Z' }],
		['updateTask', 4, { expired_at: '2026-10-01T12:30:00Z' }],
		['updateTask', 4, { expired_at: null }],
	]);
	expect(
		await code(broker.call('tasks.update', { id: 4, patch: { expired_at: '2026-10-01' } })),
	).toBe('INVALID_PARAMS');
	expect(
		await code(broker.call('tasks.update', { id: 4, patch: { expired_at: 'not a date' } })),
	).toBe('INVALID_PARAMS');
});

it('passes the new list filters and sort through, validated', async () => {
	const { broker, api } = setup(['tasks:read']);
	await broker.call('tasks.list', {
		updatedSince: '2026-09-01T00:00:00Z',
		dueBefore: '2026-10-01T00:00:00Z',
		dueAfter: '2026-08-01T00:00:00Z',
		statusType: 'active',
		priority: 'high',
		sort: 'due',
		direction: 'desc',
	});
	expect(api.calls).toEqual([
		[
			'listTasks',
			{
				statusId: null,
				categoryId: null,
				search: null,
				page: 1,
				perPage: 50,
				updatedSince: '2026-09-01T00:00:00Z',
				dueBefore: '2026-10-01T00:00:00Z',
				dueAfter: '2026-08-01T00:00:00Z',
				statusType: 'active',
				priority: 'high',
				sort: 'due',
				direction: 'desc',
			},
		],
	]);
	expect(await code(broker.call('tasks.list', { statusType: 'nope' }))).toBe('INVALID_PARAMS');
	expect(await code(broker.call('tasks.list', { priority: 'nope' }))).toBe('INVALID_PARAMS');
	expect(await code(broker.call('tasks.list', { sort: 'nope' }))).toBe('INVALID_PARAMS');
	expect(await code(broker.call('tasks.list', { direction: 'nope' }))).toBe('INVALID_PARAMS');
	expect(await code(broker.call('tasks.list', { dueBefore: 'nope' }))).toBe('INVALID_PARAMS');
});

describe('statuses and categories writes', () => {
	it('creates and updates statuses behind statuses:write, validated', async () => {
		const { broker, api } = setup(['statuses:write']);
		await broker.call('statuses.create', { name: 'Blocked', type: 'active', color: '#ff0000' });
		await broker.call('statuses.update', { id: 3, patch: { name: 'Blocked v2' } });
		await broker.call('statuses.reorder', { ids: [3, 1, 2] });
		expect(api.calls).toEqual([
			['createStatus', { name: 'Blocked', type: 'active', color: '#ff0000' }],
			['updateStatus', 3, { name: 'Blocked v2' }],
			['reorderStatuses', [3, 1, 2]],
		]);
		expect(
			await code(broker.call('statuses.create', { name: 'x', type: 'not-a-type' })),
		).toBe('INVALID_PARAMS');
		expect(
			await code(broker.call('statuses.create', { name: 'x', type: 'active', color: 'red' })),
		).toBe('INVALID_PARAMS');
		expect(await code(broker.call('statuses.reorder', { ids: [] }))).toBe('INVALID_PARAMS');
		expect(await code(broker.call('statuses.reorder', { ids: 'nope' }))).toBe('INVALID_PARAMS');

		const { broker: reader } = setup(['statuses:read']);
		expect(
			await code(reader.call('statuses.create', { name: 'x', type: 'active' })),
		).toBe('PERMISSION_DENIED');
	});

	it('creates and updates categories behind categories:write, validated', async () => {
		const { broker, api } = setup(['categories:write']);
		await broker.call('categories.create', { title: 'Backend', code: 'BE' });
		await broker.call('categories.update', { id: 5, patch: { title: 'Backend team' } });
		expect(api.calls).toEqual([
			['createCategory', { title: 'Backend', code: 'BE' }],
			['updateCategory', 5, { title: 'Backend team' }],
		]);
		expect(await code(broker.call('categories.create', { title: '' }))).toBe('INVALID_PARAMS');
		expect(
			await code(broker.call('categories.create', { title: 'x', code: 'be' })),
		).toBe('INVALID_PARAMS');
		expect(
			await code(broker.call('categories.create', { title: 'x', code: '1BE' })),
		).toBe('INVALID_PARAMS');
	});
});

describe('per-task plugin data', () => {
	it('needs tasks:read (a plugin without it could otherwise probe task ids), then round-trips JSON', async () => {
		const stored = new Map<string, string>();
		const api = {
			...fakeApi(),
			taskDataGet: async (taskId: number, key: string) => stored.get(`${taskId}:${key}`) ?? null,
			taskDataSet: async (taskId: number, key: string, json: string) => {
				stored.set(`${taskId}:${key}`, json);
			},
			taskDataDelete: async (taskId: number, key: string) => {
				stored.delete(`${taskId}:${key}`);
			},
			taskDataGetMany: async (taskIds: number[], key: string) => {
				const result: Record<number, string> = {};
				for (const id of taskIds) {
					const value = stored.get(`${id}:${key}`);
					if (value !== undefined) result[id] = value;
				}
				return result;
			},
		};
		expect(await code(setup([], { api }).broker.call('taskData.get', { taskId: 1, key: 'estimate' }))).toBe(
			'PERMISSION_DENIED',
		);
		const { broker } = setup(['tasks:read'], { api });
		expect(await broker.call('taskData.get', { taskId: 1, key: 'estimate' })).toBeNull();
		await broker.call('taskData.set', { taskId: 1, key: 'estimate', value: { points: 5 } });
		expect(await broker.call('taskData.get', { taskId: 1, key: 'estimate' })).toEqual({ points: 5 });
		expect(await broker.call('taskData.getMany', { taskIds: [1, 2], key: 'estimate' })).toEqual({
			1: { points: 5 },
		});
		await broker.call('taskData.delete', { taskId: 1, key: 'estimate' });
		expect(await broker.call('taskData.get', { taskId: 1, key: 'estimate' })).toBeNull();
	});

	it('rejects an oversized value and a key that is too long, byte-accurate for multi-byte text', async () => {
		const { broker } = setup(['tasks:read']);
		expect(
			await code(
				broker.call('taskData.set', { taskId: 1, key: 'k', value: 'x'.repeat(70_000) }),
			),
		).toBe('INVALID_PARAMS');
		expect(
			await code(
				broker.call('taskData.set', { taskId: 1, key: 'k', value: '字'.repeat(22_000) }),
			),
		).toBe('INVALID_PARAMS');
		expect(
			await code(
				broker.call('taskData.set', { taskId: 1, key: 'k'.repeat(201), value: 1 }),
			),
		).toBe('INVALID_PARAMS');
		expect(await code(broker.call('taskData.getMany', { taskIds: [], key: 'k' }))).toBe(
			'INVALID_PARAMS',
		);
		expect(
			await code(
				broker.call('taskData.getMany', {
					taskIds: Array.from({ length: 501 }, (_, i) => i + 1),
					key: 'k',
				}),
			),
		).toBe('INVALID_PARAMS');
	});
});

describe('agent work', () => {
	it('gates reads and writes behind agent_work permissions, and validates start/update/finish', async () => {
		const { broker, api } = setup(['agent_work:read']);
		expect(await code(broker.call('agentWork.list', { taskId: 1 }))).toBe('ok');
		expect(
			await code(
				broker.call('agentWork.start', { taskId: 1, agent: 'claude-code' }),
			),
		).toBe('PERMISSION_DENIED');
		expect(api.calls).toEqual([['listAgentWork', 1]]);

		const { broker: writer, api: writerApi } = setup(['agent_work:write']);
		await writer.call('agentWork.start', {
			taskId: 1,
			agent: 'claude-code',
			model: 'opus',
			sessionId: 'sess-1',
			branch: 'feat/x',
		});
		await writer.call('agentWork.update', { runId: 9, patch: { branch: 'feat/y', summary: 'wip' } });
		await writer.call('agentWork.finish', {
			runId: 9,
			patch: { status: 'succeeded', summary: 'done', branch: 'ignored' },
		});
		expect(writerApi.calls).toEqual([
			[
				'startAgentWork',
				1,
				{ agent: 'claude-code', model: 'opus', sessionId: 'sess-1', branch: 'feat/x' },
			],
			['updateAgentWork', 9, { branch: 'feat/y', summary: 'wip' }],
			['finishAgentWork', 9, { status: 'succeeded', summary: 'done' }],
		]);

		await writer.call('agentWork.start', { taskId: 1 });
		expect(writerApi.calls[writerApi.calls.length - 1]).toEqual([
			'startAgentWork',
			1,
			{ agent: undefined, model: null, sessionId: null, branch: null },
		]);

		expect(
			await code(writer.call('agentWork.start', { taskId: 1, agent: 'Claude Code!' })),
		).toBe('INVALID_PARAMS');
		expect(
			await code(writer.call('agentWork.start', { taskId: 1, agent: 'x'.repeat(41) })),
		).toBe('INVALID_PARAMS');
		expect(
			await code(
				writer.call('agentWork.finish', { runId: 9, patch: { status: 'closed' } }),
			),
		).toBe('INVALID_PARAMS');
		expect(
			await code(
				writer.call('agentWork.update', {
					runId: 9,
					patch: { prUrl: 'ftp://example.com/x' },
				}),
			),
		).toBe('INVALID_PARAMS');
		expect(
			await code(
				writer.call('agentWork.update', {
					runId: 9,
					patch: { commits: [{ sha: 'zz', message: 'bad sha' }] },
				}),
			),
		).toBe('INVALID_PARAMS');
		expect(
			await code(
				writer.call('agentWork.update', {
					runId: 9,
					patch: { tests: { passed: -1 } },
				}),
			),
		).toBe('INVALID_PARAMS');
	});

	it('rejects an agent label that would push "plugin:<id>/<label>" past 64 characters, never truncating it', async () => {
		const longId = `${'p'.repeat(24)}.${'q'.repeat(24)}`; // "plugin:" + 49 = 56; 7 bytes of budget left
		const longManifest = parseManifest({
			network: { allowedOrigins: [] },
			id: longId,
			name: 'Long',
			version: '1.0.0',
			engines: { tmgr: '^1.0' },
			permissions: ['agent_work:write'],
			contributes: {
				boardCardBadges: [],
				statusBarItems: [],
				commands: [],
				views: [],
				taskPanelSections: [],
			},
		});
		const { broker, api } = setup(['agent_work:write'], { manifest: longManifest });
		expect(
			await code(broker.call('agentWork.start', { taskId: 1, agent: 'a'.repeat(7) })),
		).toBe('ok');
		expect(api.calls[0]).toEqual([
			'startAgentWork',
			1,
			{ agent: 'a'.repeat(7), model: null, sessionId: null, branch: null },
		]);
		expect(
			await code(broker.call('agentWork.start', { taskId: 1, agent: 'a'.repeat(8) })),
		).toBe('INVALID_PARAMS');
	});
});

describe('setViewBadge', () => {
	const badges = () => {
		const calls: unknown[][] = [];
		return { calls, setViewBadge: (id: string, badge: unknown) => void calls.push([id, badge]) };
	};
	const call = (broker: ReturnType<typeof setup>['broker'], viewId: unknown, badge: unknown) =>
		broker.call('ui.setViewBadge', { viewId, badge });

	it('needs the views:badge permission', async () => {
		const { calls, setViewBadge } = badges();
		const { broker } = setup([], { setViewBadge });
		expect(await code(call(broker, 'report', { count: 1 }))).toBe('PERMISSION_DENIED');
		expect(calls).toEqual([]);
	});

	it('takes only views this plugin declares, answering INVALID_PARAMS for any other', async () => {
		const { calls, setViewBadge } = badges();
		const { broker } = setup(['views:badge'], { setViewBadge });
		for (const viewId of ['other', 'tmgr.other:report', '', 5, null, undefined])
			expect(await code(call(broker, viewId, { count: 1 }))).toBe('INVALID_PARAMS');
		expect(calls).toEqual([]);
	});

	it('sets a count or a text, defaulting the tone', async () => {
		const { calls, setViewBadge } = badges();
		const { broker } = setup(['views:badge'], { setViewBadge });
		expect(await call(broker, 'report', { count: 12 })).toBeNull();
		await call(broker, 'report', { text: 'new', tone: 'danger' });
		await call(broker, 'report', { count: 1e6, tone: 'warning' });
		await call(broker, 'report', { text: '😀😀', tone: 'info' });
		expect(calls).toEqual([
			['report', { count: 12, text: null, tone: 'default' }],
			['report', { count: null, text: 'new', tone: 'danger' }],
			['report', { count: 1e6, text: null, tone: 'warning' }],
			['report', { count: null, text: '😀😀', tone: 'info' }],
		]);
	});

	it('clears on null and on a count of 0', async () => {
		const { calls, setViewBadge } = badges();
		const { broker } = setup(['views:badge'], { setViewBadge });
		await call(broker, 'report', null);
		await call(broker, 'report', { count: 0, tone: 'danger' });
		expect(calls).toEqual([
			['report', null],
			['report', null],
		]);
	});

	it.each([
		['both count and text', { count: 1, text: 'a' }],
		['neither count nor text', {}],
		['only a tone', { tone: 'info' }],
		['a negative count', { count: -1 }],
		['a fractional count', { count: 1.5 }],
		['an unsafe count', { count: 2 ** 53 }],
		['NaN', { count: NaN }],
		['a numeric string count', { count: '3' }],
		['a null count', { count: null }],
		['an empty text', { text: '' }],
		['a text of 5 units', { text: 'abcde' }],
		['a 3-emoji text (6 units)', { text: '😀😀😀' }],
		['a numeric text', { text: 4 }],
		['an unknown tone', { count: 1, tone: 'green' }],
		['a non-string tone', { count: 1, tone: 3 }],
		['an array', [1]],
		['a string', 'x'],
		['a number', 3],
		['undefined', undefined],
	])('refuses %s with INVALID_PARAMS', async (_, badge) => {
		const { calls, setViewBadge } = badges();
		const { broker } = setup(['views:badge'], { setViewBadge });
		expect(await code(call(broker, 'report', badge))).toBe('INVALID_PARAMS');
		expect(calls).toEqual([]);
	});
});

describe('tray', () => {
	const tray = () => {
		const items: unknown[] = [];
		const titles: unknown[] = [];
		let owner = true;
		return {
			items,
			titles,
			setOwner: (value: boolean) => (owner = value),
			tray: {
				setItem: (id: string, item: unknown) => void items.push([id, item]),
				setTitle: (text: string | null) => void titles.push(text),
				isTitleOwner: () => owner,
			},
		};
	};

	it('needs the tray permission and machine access, and validates the item', async () => {
		const { tray: deps, items } = tray();
		expect(
			await code(setup([], { tray: deps }).broker.call('ui.setTrayItem', { id: 'menu', title: 'x', items: [] })),
		).toBe('PERMISSION_DENIED');
		const { broker } = setup(['tray'], { tray: deps });
		expect(
			await code(broker.call('ui.setTrayItem', { id: 'other', title: 'x', items: [] })),
		).toBe('NOT_DECLARED');
		expect(
			await code(
				broker.call('ui.setTrayItem', {
					id: 'menu',
					title: 'x',
					items: Array.from({ length: 11 }, () => ({ title: 'a' })),
				}),
			),
		).toBe('INVALID_PARAMS');
		expect(
			await code(
				broker.call('ui.setTrayItem', {
					id: 'menu',
					title: 'x',
					items: [{ title: 'Go', command: 'not.declared' }],
				}),
			),
		).toBe('NOT_DECLARED');
		expect(
			await broker.call('ui.setTrayItem', {
				id: 'menu',
				title: 'Section',
				items: [{ title: 'Go', command: 'tmgr.test.go', taskId: 4, args: { a: 1 } }],
			}),
		).toBeNull();
		expect(await broker.call('ui.setTrayItem', { id: 'menu', items: null })).toBeNull();
		expect(items).toEqual([
			[
				'menu',
				{
					title: 'Section',
					items: [{ title: 'Go', taskId: 4, command: 'tmgr.test.go', args: { a: 1 } }],
				},
			],
			['menu', null],
		]);
		const { broker: noTray } = setup(['tray']);
		expect(
			await code(noTray.call('ui.setTrayItem', { id: 'menu', title: 'x', items: [] })),
		).toBe('PERMISSION_DENIED');
	});

	it('accepts emoji-heavy titles and shortens them to 60 graphemes with an ellipsis', async () => {
		const { tray: deps, items } = tray();
		const { broker } = setup(['tray'], { tray: deps });
		const long = '😀'.repeat(45);
		await broker.call('ui.setTrayItem', {
			id: 'menu',
			title: long,
			items: [{ title: `${'a'.repeat(59)}b` }, { title: `${'👨‍👩‍👧'.repeat(61)}` }],
		});
		const [, spec] = items[0] as [string, any];
		expect(spec.title).toBe(long);
		expect(spec.items[0].title).toBe(`${'a'.repeat(59)}b`);
		expect(spec.items[1].title).toBe(`${'👨‍👩‍👧'.repeat(59)}…`);
		await broker.call('ui.setTrayItem', {
			id: 'menu',
			title: '😀'.repeat(70),
			items: [],
		});
		expect((items[1] as [string, any])[1].title).toBe(`${'😀'.repeat(59)}…`);
	});

	it('still refuses empty, non-string and absurdly long titles', async () => {
		const { tray: deps } = tray();
		const { broker } = setup(['tray'], { tray: deps });
		for (const title of ['', '  ', 5, null, 'x'.repeat(1001)])
			expect(await code(broker.call('ui.setTrayItem', { id: 'menu', title, items: [] }))).toBe(
				'INVALID_PARAMS',
			);
	});

	it('lets only the chosen plugin set the menu bar title, within the length and shape limits', async () => {
		const { tray: deps, titles, setOwner } = tray();
		const { broker } = setup(['tray'], { tray: deps });
		expect(await broker.call('ui.setTrayTitle', { text: ' 3 tasks ' })).toBeNull();
		expect(await code(broker.call('ui.setTrayTitle', { text: 'way too long text' }))).toBe(
			'INVALID_PARAMS',
		);
		expect(await code(broker.call('ui.setTrayTitle', { text: 'a\nb' }))).toBe('INVALID_PARAMS');
		expect(await code(broker.call('ui.setTrayTitle', { text: 'a\u0000b' }))).toBe(
			'INVALID_PARAMS',
		);
		expect(await broker.call('ui.setTrayTitle', { text: null })).toBeNull();
		setOwner(false);
		expect(await code(broker.call('ui.setTrayTitle', { text: 'x' }))).toBe('PERMISSION_DENIED');
		expect(titles).toEqual(['3 tasks', null]);
	});
});

describe('files', () => {
	const files = () => {
		const done: unknown[] = [];
		return {
			done,
			files: {
				export: async (path: string, content: string) => {
					done.push(['export', path, content.length]);
					return { path };
				},
				reveal: async (path: string) => void done.push(['reveal', path]),
				pick: async () => ({
					name: 'notes.txt',
					size: 2,
					base64: 'aGk=',
					text: 'hi',
				}),
			},
		};
	};

	it('writes exports only inside the plugin folder', async () => {
		const { done, files: deps } = files();
		const { broker } = setup(['files:export'], { files: deps });
		expect(
			await broker.call('files.export', {
				path: 'reports/week 39.md',
				content: '# Week',
			}),
		).toEqual({
			path: 'reports/week 39.md',
		});
		for (const path of [
			'../escape.md',
			'run.command',
			'page.webloc',
			'noext',
			'/etc/passwd',
			'a/../../b',
			'a\\\\b',
			'',
			'a/b/c/d/e/f.md',
		]) {
			expect(
				await code(broker.call('files.export', { path, content: 'x' })),
			).toBe('INVALID_PARAMS');
		}
		expect(
			await code(
				broker.call('files.export', {
					path: 'big.md',
					content: 'x'.repeat(5 * 1024 * 1024 + 1),
				}),
			),
		).toBe('INVALID_PARAMS');
		await broker.call('files.reveal', { path: 'reports/week 39.md' });
		expect(done).toEqual([
			['export', 'reports/week 39.md', 6],
			['reveal', 'reports/week 39.md'],
		]);
	});

	it('needs a permission for each kind of file access', async () => {
		const { files: deps } = files();
		const { broker } = setup([], { files: deps });
		expect(
			await code(broker.call('files.export', { path: 'a.md', content: '' })),
		).toBe('PERMISSION_DENIED');
		expect(await code(broker.call('files.list', { taskId: 1 }))).toBe(
			'PERMISSION_DENIED',
		);
		expect(await code(broker.call('files.read', { fileId: 1 }))).toBe(
			'PERMISSION_DENIED',
		);
		expect(await code(broker.call('files.pick', {}))).toBe('PERMISSION_DENIED');
		const { broker: picker } = setup(['files:pick'], { files: deps });
		expect(await picker.call('files.pick', {})).toEqual({
			name: 'notes.txt',
			size: 2,
			base64: 'aGk=',
			text: 'hi',
		});
	});

	it('reads attachments of the plugin workspace', async () => {
		const { broker, api } = setup(['files:attachments']);
		await broker.call('files.list', { taskId: 4 });
		await broker.call('files.read', { fileId: 9 });
		expect(api.calls).toEqual([
			['listAttachments', 4],
			['readAttachment', 9],
		]);
	});
});

describe('localAccess.requestConnection', () => {
	const withCompanion = { ...manifest([]), companion: { description: 'A CLI' } };

	it('refuses without a companion section in the manifest', async () => {
		const { broker } = setup([], { manifest: manifest([]) });
		expect(await code(broker.call('localAccess.requestConnection', {}))).toBe(
			'PERMISSION_DENIED',
		);
	});

	it('refuses in a shared (cloud) workspace', async () => {
		const { broker } = setup([], {
			manifest: withCompanion,
			workspace: { id: -7, code: 'shared', name: 'Shared', kind: 'cloud' },
		});
		expect(await code(broker.call('localAccess.requestConnection', {}))).toBe(
			'NOT_SUPPORTED',
		);
	});

	it('fails with HOST_ERROR when the host offers no connect flow', async () => {
		const { broker } = setup([], { manifest: withCompanion });
		expect(await code(broker.call('localAccess.requestConnection', {}))).toBe(
			'HOST_ERROR',
		);
	});

	it('resolves with only tokenId and prefix, forwarding label and permissions', async () => {
		const requests: unknown[] = [];
		const { broker } = setup([], {
			manifest: withCompanion,
			localAccess: {
				requestConnection: async (opts) => {
					requests.push(opts);
					return { status: 'connected', tokenId: 'lt_abc', prefix: 'tmgrl_1234' };
				},
			},
		});
		const result = await broker.call('localAccess.requestConnection', {
			label: 'My CLI',
			permissions: ['tasks:read'],
		});
		expect(result).toEqual({
			status: 'connected',
			tokenId: 'lt_abc',
			prefix: 'tmgrl_1234',
		});
		expect(requests).toEqual([{ label: 'My CLI', permissions: ['tasks:read'] }]);
	});

	it('allows a cancelled result through unchanged', async () => {
		const { broker } = setup([], {
			manifest: withCompanion,
			localAccess: { requestConnection: async () => ({ status: 'cancelled' }) },
		});
		expect(await broker.call('localAccess.requestConnection', {})).toEqual({
			status: 'cancelled',
		});
	});

	it('rate limits to one pending request per plugin', async () => {
		let release: (() => void) | null = null;
		const pending = new Promise<void>((resolve) => (release = resolve));
		const { broker } = setup([], {
			manifest: withCompanion,
			localAccess: {
				requestConnection: async () => {
					await pending;
					return { status: 'cancelled' };
				},
			},
		});
		const first = broker.call('localAccess.requestConnection', {});
		expect(await code(broker.call('localAccess.requestConnection', {}))).toBe(
			'RATE_LIMITED',
		);
		release!();
		await first;
		expect(await code(broker.call('localAccess.requestConnection', {}))).toBe(
			'ok',
		);
	});
});

describe('routines', () => {
	it('gates reads and writes behind routines permissions, validated', async () => {
		const { broker, api } = setup(['routines:read']);
		await broker.call('routines.list', { from: '2026-09-01', to: '2026-09-30' });
		await broker.call('routines.get', { id: 4 });
		await broker.call('routines.instances', { id: 4 });
		expect(api.calls).toEqual([
			['listRoutines', '2026-09-01', '2026-09-30'],
			['getRoutine', 4],
			['listRoutineInstances', 4],
		]);
		expect(
			await code(broker.call('routines.create', { title: 'Read' })),
		).toBe('PERMISSION_DENIED');

		const { broker: writer, api: writerApi } = setup(['routines:write']);
		await writer.call('routines.create', { title: 'Read', date: '2026-09-26', time: '09:00' });
		await writer.call('routines.update', { id: 4, patch: { title: 'New' } });
		await writer.call('routines.complete', { id: 4, date: '2026-09-26' });
		await writer.call('routines.skip', { id: 4 });
		expect(writerApi.calls).toEqual([
			['createRoutine', { title: 'Read', description: null, date: '2026-09-26', time: '09:00' }],
			['updateRoutine', 4, { title: 'New' }],
			['completeRoutine', 4, '2026-09-26'],
			['skipRoutine', 4, expect.stringMatching(/^\d{4}-\d{2}-\d{2}$/)],
		]);
		expect(await code(writer.call('routines.list', { from: '2026-09-01', to: '2026-09-30' }))).toBe(
			'PERMISSION_DENIED',
		);
	});

	it('validates the date range, dates, times and update patch', async () => {
		const { broker } = setup(['routines:read']);
		expect(await code(broker.call('routines.list', { from: '2026-09-30', to: '2026-09-01' }))).toBe(
			'INVALID_PARAMS',
		);
		expect(await code(broker.call('routines.list', { from: '2026-01-01', to: '2026-12-31' }))).toBe(
			'INVALID_PARAMS',
		);
		expect(await code(broker.call('routines.list', { from: '2026-02-30', to: '2026-02-30' }))).toBe(
			'INVALID_PARAMS',
		);
		expect(await code(broker.call('routines.list', { from: 'nope', to: '2026-09-30' }))).toBe(
			'INVALID_PARAMS',
		);

		const { broker: writer } = setup(['routines:write']);
		expect(await code(writer.call('routines.create', { title: '' }))).toBe('INVALID_PARAMS');
		expect(
			await code(writer.call('routines.create', { title: 'x', time: '09:00' })),
		).toBe('INVALID_PARAMS');
		expect(
			await code(writer.call('routines.create', { title: 'x', date: '2026-09-26', time: '9:00' })),
		).toBe('INVALID_PARAMS');
		expect(await code(writer.call('routines.update', { id: 4, patch: {} }))).toBe('INVALID_PARAMS');
		expect(await code(writer.call('routines.update', { id: 4, patch: 'title' }))).toBe('INVALID_PARAMS');
	});

	it('refuses every routines.* call outside a local workspace, before touching the api', async () => {
		const { broker, api } = setup(['routines:read', 'routines:write'], {
			workspace: { id: -7, code: 'shared', name: 'Shared', kind: 'cloud' },
		});
		expect(
			await code(broker.call('routines.list', { from: '2026-09-01', to: '2026-09-02' })),
		).toBe('NOT_SUPPORTED');
		expect(await code(broker.call('routines.get', { id: 1 }))).toBe('NOT_SUPPORTED');
		expect(await code(broker.call('routines.create', { title: 'x' }))).toBe('NOT_SUPPORTED');
		expect(await code(broker.call('routines.complete', { id: 1 }))).toBe('NOT_SUPPORTED');
		expect(
			await code(broker.call('routines.convertToTask', { id: 1 })),
		).toBe('NOT_SUPPORTED');
		expect(api.calls).toEqual([]);
	});

	it('convertToTask needs both routines:write and tasks:write', async () => {
		const { broker, api } = setup(['routines:write']);
		expect(await code(broker.call('routines.convertToTask', { id: 4 }))).toBe(
			'PERMISSION_DENIED',
		);
		expect(api.calls).toEqual([]);

		const { broker: allowed, api: allowedApi } = setup(['routines:write', 'tasks:write']);
		await allowed.call('routines.convertToTask', {
			id: 4,
			categoryId: 9,
			statusId: 2,
		});
		expect(allowedApi.calls).toEqual([
			['convertRoutine', 4, { categoryId: 9, statusId: 2 }],
		]);
	});

	it('refuses tasks.get and tasks.update for a routine-range id in a local workspace', async () => {
		const { broker, api } = setup(['tasks:read', 'tasks:write']);
		expect(await code(broker.call('tasks.get', { id: 1_000_000_005 }))).toBe(
			'INVALID_PARAMS',
		);
		expect(
			await code(broker.call('tasks.update', { id: 1_000_000_005, patch: { title: 'x' } })),
		).toBe('INVALID_PARAMS');
		expect(api.calls).toEqual([]);
		await broker.call('tasks.get', { id: 4 });
		expect(api.calls).toEqual([['getTask', 4]]);
	});

	it('does not refuse routine-range task ids outside a local workspace (the server owns that check there)', async () => {
		const { broker, api } = setup(['tasks:read'], {
			workspace: { id: -7, code: 'shared', name: 'Shared', kind: 'cloud' },
		});
		await broker.call('tasks.get', { id: 1_000_000_005 });
		expect(api.calls).toEqual([['getTask', 1_000_000_005]]);
	});
});

describe('pages (API 1.5)', () => {
	it('gates reads, writes and sections by their own permission', async () => {
		const read = setup(['pages:read']).broker;
		expect(await code(read.call('pages.search', { q: 'x' }))).toBe('ok');
		expect(await code(read.call('pages.tree', {}))).toBe('ok');
		expect(await code(read.call('pages.get', { idOrSlug: 'saha' }))).toBe('ok');
		expect(await code(read.call('pages.create', { title: 'T' }))).toBe('PERMISSION_DENIED');
		expect(await code(read.call('pages.setSection', { id: 5, sectionId: 'mine', markdown: 'x' }))).toBe(
			'PERMISSION_DENIED',
		);
		const write = setup(['pages:read', 'pages:write']).broker;
		expect(await code(write.call('pages.append', { id: 5, markdown: 'line' }))).toBe('ok');
		expect(await code(write.call('pages.setSection', { id: 5, sectionId: 'mine', markdown: 'x' }))).toBe(
			'PERMISSION_DENIED',
		);
		expect(await code(setup([]).broker.call('pageData.get', { pageId: 5, key: 'k' }))).toBe('PERMISSION_DENIED');
	});

	it('validates and maps the parameters', async () => {
		const { broker, api } = setup(['pages:read', 'pages:write']);
		await broker.call('pages.create', {
			title: 'Saha',
			type: 'person',
			parentId: 3,
			body: 'hi',
			properties: { role: 'cto' },
		});
		await broker.call('pages.update', { id: 5, version: 2, title: 'N', summary: 's' });
		await broker.call('pages.append', { id: 5, markdown: 'm', heading: 'Log', createHeading: true });
		await broker.call('pages.search', { q: 'x', type: 'person', limit: 500 });
		expect(api.calls).toEqual([
			['pagesCreate', { title: 'Saha', type: 'person', parent_id: 3, body: 'hi', properties: { role: 'cto' } }],
			['pagesUpdate', 5, { version: 2, title: 'N', summary: 's' }],
			['pagesAppend', 5, { markdown: 'm', heading: 'Log', create_heading: true }],
			['pagesSearch', 'x', { type: 'person', limit: 50 }],
		]);
		expect(await code(broker.call('pages.update', { id: 5, title: 'x' }))).toBe('INVALID_PARAMS');
		expect(await code(broker.call('pages.create', { title: '' }))).toBe('INVALID_PARAMS');
		expect(await code(broker.call('pages.create', { title: 'x', type: 'weird' }))).toBe('INVALID_PARAMS');
		expect(await code(broker.call('pages.get', { idOrSlug: 0 }))).toBe('INVALID_PARAMS');
		expect(await code(broker.call('pages.append', { id: 5, markdown: 'x'.repeat(1_048_577) }))).toBe(
			'INVALID_PARAMS',
		);
		expect(await code(broker.call('pages.create', { title: 'x', properties: [] }))).toBe('INVALID_PARAMS');
	});

	it('lets setSection write only into sections owned by this plugin', async () => {
		const { broker, api } = setup(['pages:read', 'pages:sections']);
		expect(await code(broker.call('pages.setSection', { id: 5, sectionId: 'mine', markdown: 'ok' }))).toBe('ok');
		expect(api.calls[api.calls.length - 1]).toEqual(['pagesSetSection', 5, 'mine', 'ok', undefined, undefined]);
		const before = api.calls.length;
		expect(await code(broker.call('pages.setSection', { id: 5, sectionId: 'theirs', markdown: 'x' }))).toBe(
			'PERMISSION_DENIED',
		);
		expect(await code(broker.call('pages.setSection', { id: 5, sectionId: 'shared', markdown: 'x' }))).toBe(
			'PERMISSION_DENIED',
		);
		expect(await code(broker.call('pages.setSection', { id: 5, sectionId: 'a b', markdown: 'x' }))).toBe(
			'INVALID_PARAMS',
		);
		expect(await code(broker.call('pages.setSection', { id: 5, sectionId: 'x', markdown: 'x', heading: 'h'.repeat(201) }))).toBe(
			'INVALID_PARAMS',
		);
		expect(api.calls.slice(before).filter((c) => c[0] === 'pagesSetSection')).toEqual([]);
	});

	it('lets setSection create a section the page does not have yet, with an optional heading', async () => {
		const { broker, api } = setup(['pages:read', 'pages:sections']);
		expect(
			await code(broker.call('pages.setSection', { id: 5, sectionId: 'nope', markdown: 'x', heading: 'Notes' })),
		).toBe('ok');
		expect(api.calls[api.calls.length - 1]).toEqual(['pagesSetSection', 5, 'nope', 'x', undefined, 'Notes']);
	});

	it('stores per-page data as JSON with a 200-character key and a 64 KB value', async () => {
		const { broker, api } = setup(['pages:read']);
		await broker.call('pageData.set', { pageId: 5, key: 'seen', value: { n: 1 } });
		expect(api.calls[api.calls.length - 1]).toEqual(['pageDataSet', 5, 'seen', '{"n":1}']);
		expect(await code(broker.call('pageData.set', { pageId: 5, key: 'k'.repeat(201), value: 1 }))).toBe(
			'INVALID_PARAMS',
		);
		expect(await code(broker.call('pageData.set', { pageId: 5, key: 'k', value: 'x'.repeat(70_000) }))).toBe(
			'INVALID_PARAMS',
		);
		expect(await broker.call('pageData.getMany', { pageIds: [5, 6], key: 'k' })).toEqual({ 5: { a: 1 }, 6: 2 });
		expect(await code(broker.call('pageData.getMany', { pageIds: [], key: 'k' }))).toBe('INVALID_PARAMS');
	});

	it('subscribes to page events only with pages:read', async () => {
		expect(await code(setup(['pages:read']).broker.call('register', { kind: 'event', id: 'page.updated' }))).toBe(
			'ok',
		);
		expect(await code(setup(['tasks:read']).broker.call('register', { kind: 'event', id: 'page.updated' }))).toBe(
			'PERMISSION_DENIED',
		);
	});
});
