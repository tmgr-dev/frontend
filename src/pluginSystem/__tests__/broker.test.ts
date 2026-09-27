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
		listAgentWork: record('listAgentWork'),
		startAgentWork: record('startAgentWork'),
		updateAgentWork: record('updateAgentWork'),
		finishAgentWork: record('finishAgentWork'),
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
