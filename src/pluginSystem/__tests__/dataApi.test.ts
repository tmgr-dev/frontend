import axios from 'axios';
import { createDataApi } from '../dataApi';

const recording = (answer: (method: string, url: string) => unknown) => {
	const seen: string[] = [];
	const http = axios.create({
		adapter: async (config) => {
			seen.push(
				`${config.method?.toUpperCase()} ${config.url}${
					config.params ? ' ' + JSON.stringify(config.params) : ''
				} ${config.headers['X-TMGR-Plugin']}${
					config.data ? ' ' + config.data : ''
				}`,
			);
			return {
				data: answer(config.method!, config.url!),
				status: 200,
				statusText: 'OK',
				headers: {},
				config,
			};
		},
	});
	return { http, seen };
};

it('maps plugin calls to the app API, marked as the plugin', async () => {
	const { http, seen } = recording((_method, url) =>
		url === 'tasks'
			? { data: [{ id: 1 }], meta: { total: 7 } }
			: url.includes('/storage/')
			? { data: { value: '{"a":1}' } }
			: { data: { id: 9 } },
	);
	const api = createDataApi(http, 'tmgr.estimate');
	expect(
		await api.listTasks({
			statusId: 3,
			categoryId: null,
			search: null,
			page: 1,
			perPage: 50,
		}),
	).toEqual({
		items: [{ id: 1, key: null }],
		total: 7,
	});
	expect(await api.updateTask(4, { title: 'x' })).toEqual({ id: 9, key: null });
	expect(await api.storageGet('last run')).toBe('{"a":1}');
	await api.addComment(4, 'hi');
	expect(seen).toEqual([
		'GET tasks {"page":1,"per_page":50,"status_id":3} tmgr.estimate',
		'PATCH tasks/4 tmgr.estimate {"title":"x"}',
		'GET plugins/tmgr.estimate/storage/last%20run tmgr.estimate',
		'POST tasks/4/comments tmgr.estimate {"message":"hi"}',
	]);
});

it('lists and reads attachments with text and base64', async () => {
	const { http, seen } = recording((_method, url) =>
		url === 'tasks/4/files'
			? {
					data: [
						{
							id: 9,
							name: 'notes.md',
							mime_type: 'text/markdown',
							size: 4,
							created_at: 'x',
						},
					],
			  }
			: url === 'files/9'
			? {
					data: {
						id: 9,
						name: 'notes.md',
						mime_type: 'text/markdown',
						size: 4,
					},
			  }
			: new Blob(['# Hi']),
	);
	const api = createDataApi(http, 'tmgr.files');
	expect(await api.listAttachments(4)).toEqual([
		{
			id: 9,
			name: 'notes.md',
			mimeType: 'text/markdown',
			size: 4,
			createdAt: 'x',
		},
	]);
	expect(await api.readAttachment(9)).toEqual({
		name: 'notes.md',
		mimeType: 'text/markdown',
		size: 4,
		base64: 'IyBIaQ==',
		text: '# Hi',
	});
	expect(seen.map((line) => line.split(' ').slice(0, 2).join(' '))).toEqual([
		'GET tasks/4/files',
		'GET files/9',
		'GET files/9/content',
	]);
});

it('leaves cloud comments without a server author as unknown (null), but passes one through if present', async () => {
	const { http } = recording((method, url) =>
		url === 'tasks/4/comments' && method === 'get'
			? { data: [{ id: 1, userId: 9, message: 'hi' }, { id: 2, author: { kind: 'plugin', id: 'x', name: 'X' }, message: 'yo' }] }
			: { data: { id: 3, userId: 9, message: 'new' } },
	);
	const api = createDataApi(http, 'tmgr.estimate');
	expect(await api.listComments(4)).toEqual([
		{ id: 1, userId: 9, message: 'hi', author: null },
		{ id: 2, author: { kind: 'plugin', id: 'x', name: 'X' }, message: 'yo' },
	]);
});

it('names addComment\'s own author as this plugin, since we know it even when the server does not say so', async () => {
	const { http } = recording(() => ({ data: { id: 3, userId: 9, message: 'new' } }));
	const api = createDataApi(http, 'tmgr.estimate', 'tmgr.estimate', 'Estimate');
	expect(await api.addComment(4, 'new')).toEqual({
		id: 3,
		userId: 9,
		message: 'new',
		author: { kind: 'plugin', id: 'tmgr.estimate', name: 'Estimate' },
	});
});

it('resolves a relation type name to its fixed id, without asking the server', async () => {
	const { http, seen } = recording((_method, url) => {
		if (url === 'task-relation-types') throw new Error('must not fetch relation types');
		if (url === 'tasks/4/relations') {
			return {
				data: [
					{ id: 1, relation_type: { id: 1, name: 'blocks' }, related_task: { id: 9 } },
				],
			};
		}
		return { data: { id: 1 } };
	});
	const api = createDataApi(http, 'tmgr.estimate');
	expect(await api.listRelations(4)).toEqual([{ taskId: 4, otherTaskId: 9, type: 'blocks' }]);
	await api.relateTask(4, 9, 'blocks');
	await api.unrelateTask(4, 9, 'relates to');
	expect(seen.filter((line) => line.startsWith('POST tasks/4/related-to/9/with/1'))).toHaveLength(1);
	expect(seen.filter((line) => line.startsWith('DELETE tasks/4/related-to/9/with/3'))).toHaveLength(1);
	await expect(api.relateTask(4, 9, 'not a relation type' as any)).rejects.toThrow('unknown relation type');
});

it('keeps storage in the namespace it is given', async () => {
	const { http, seen } = recording(() => ({ data: { value: null } }));
	await createDataApi(
		http,
		'acme.board',
		'acme.board@github.com/acme/board',
	).storageGet('k');
	expect(seen[0]).toBe(
		'GET plugins/acme.board%40github.com%2Facme%2Fboard/storage/k acme.board',
	);
});

it('tells the local workspace the storage id, so a different repo reusing the plugin id owns nothing of it', async () => {
	let header: unknown;
	const http = axios.create({
		adapter: async (config) => {
			header = config.headers['X-TMGR-Plugin-Storage'];
			return { data: { data: null }, status: 200, statusText: 'OK', headers: {}, config };
		},
	});
	await createDataApi(http, 'acme.board', 'acme.board@github.com/acme/board').storageGet('k');
	expect(decodeURIComponent(String(header))).toBe('acme.board@github.com/acme/board');
});

it('never sends the storage header to the cloud client', async () => {
	let header: unknown;
	const http = axios.create({
		adapter: async (config) => {
			header = config.headers['X-TMGR-Plugin-Storage'];
			return { data: { data: null }, status: 200, statusText: 'OK', headers: {}, config };
		},
	});
	await createDataApi(http, 'acme.board', 'acme.board', 'Board', true).storageGet('k');
	expect(header).toBeUndefined();
});

it('rejects clearing the due date in a shared workspace instead of silently keeping it (the server ignores null)', async () => {
	const { http, seen } = recording(() => ({ data: { id: 4 } }));
	const api = createDataApi(http, 'acme.board', 'acme.board', 'Board', true);
	await expect(api.updateTask(4, { expired_at: null })).rejects.toMatchObject({ code: 'NOT_SUPPORTED' });
	expect(seen).toEqual([]);
	await api.updateTask(4, { title: 'x' });
	expect(seen).toHaveLength(1);
});

it('sends the plugin name so a header can carry any script', async () => {
	let name: unknown;
	const http = axios.create({
		adapter: async (config) => {
			name = config.headers['X-TMGR-Plugin-Name'];
			return { data: { data: [] }, status: 200, statusText: 'OK', headers: {}, config };
		},
	});
	await createDataApi(http, 'acme.board', 'acme.board', 'Доска').listComments(1);
	expect(decodeURIComponent(String(name))).toBe('Доска');
	expect(String(name)).toMatch(/^[\x20-\x7e]+$/);
});

it('refuses task relations in shared workspaces until the server lists them', async () => {
	const { http, seen } = recording(() => ({ data: [] }));
	await expect(
		createDataApi(http, 'acme.board', 'acme.board', 'Board', true).listRelations(1),
	).rejects.toMatchObject({ code: 'NOT_SUPPORTED' });
	expect(seen).toEqual([]);
});

it('computes the ticket key from category.code and category_tasks_sequence_id', async () => {
	const { http } = recording((_method, url) =>
		url === 'tasks/4'
			? { data: { id: 4, category: { code: 'TM' }, category_tasks_sequence_id: 12 } }
			: { data: { id: 5, category: null, category_tasks_sequence_id: null } },
	);
	const api = createDataApi(http, 'tmgr.estimate');
	expect((await api.getTask(4) as any).key).toBe('TM-12');
	expect((await api.getTask(5) as any).key).toBeNull();
});

it('refuses the new task filters and sort in shared workspaces before sending a request', async () => {
	const { http, seen } = recording(() => ({ data: [] }));
	const api = createDataApi(http, 'acme.board', 'acme.board', 'Board', true);
	await expect(
		api.listTasks({
			statusId: null,
			categoryId: null,
			search: null,
			page: 1,
			perPage: 50,
			sort: 'due',
		}),
	).rejects.toMatchObject({ code: 'NOT_SUPPORTED' });
	expect(seen).toEqual([]);
	await expect(
		api.listTasks({
			statusId: null,
			categoryId: null,
			search: null,
			page: 1,
			perPage: 50,
			priority: 'high',
		}),
	).rejects.toMatchObject({ code: 'NOT_SUPPORTED' });
	expect(seen).toEqual([]);
});

it('sends the new task filters and sort to the local route', async () => {
	const { http, seen } = recording(() => ({ data: [] }));
	await createDataApi(http, 'tmgr.estimate').listTasks({
		statusId: null,
		categoryId: null,
		search: null,
		page: 1,
		perPage: 50,
		updatedSince: '2026-09-01T00:00:00Z',
		dueBefore: '2026-10-01T00:00:00Z',
		statusType: 'active',
		priority: 'high',
		sort: 'due',
		direction: 'desc',
	});
	expect(seen[0]).toBe(
		'GET tasks {"page":1,"per_page":50,"updated_since":"2026-09-01T00:00:00Z","due_before":"2026-10-01T00:00:00Z","status_type":"active","priority":"high","sort":"due","direction":"desc"} tmgr.estimate',
	);
});

it('refuses per-task plugin data in shared workspaces for all four calls', async () => {
	const { http, seen } = recording(() => ({ data: {} }));
	const api = createDataApi(http, 'acme.board', 'acme.board', 'Board', true);
	await expect(api.taskDataGet(1, 'k')).rejects.toMatchObject({ code: 'NOT_SUPPORTED' });
	await expect(api.taskDataSet(1, 'k', '1')).rejects.toMatchObject({ code: 'NOT_SUPPORTED' });
	await expect(api.taskDataDelete(1, 'k')).rejects.toMatchObject({ code: 'NOT_SUPPORTED' });
	await expect(api.taskDataGetMany([1], 'k')).rejects.toMatchObject({ code: 'NOT_SUPPORTED' });
	expect(seen).toEqual([]);
});

it('scopes per-task plugin data under the plugin storage id', async () => {
	const { http, seen } = recording((_method, url) =>
		url.endsWith('/data/k') ? { data: { value: '"v"' } } : { data: { 1: '"a"' } },
	);
	const api = createDataApi(http, 'acme.board', 'acme.board@github.com/acme/board');
	expect(await api.taskDataGet(1, 'k')).toBe('"v"');
	await api.taskDataSet(1, 'k', '"v"');
	await api.taskDataDelete(1, 'k');
	await api.taskDataGetMany([1, 2], 'k');
	expect(seen.map((line) => line.split(' ').slice(0, 2).join(' '))).toEqual([
		'GET plugins/acme.board%40github.com%2Facme%2Fboard/tasks/1/data/k',
		'PUT plugins/acme.board%40github.com%2Facme%2Fboard/tasks/1/data/k',
		'DELETE plugins/acme.board%40github.com%2Facme%2Fboard/tasks/1/data/k',
		'POST plugins/acme.board%40github.com%2Facme%2Fboard/task-data/query',
	]);
});

it('writes statuses to the workspace route and categories with workspace_id only in the cloud', async () => {
	const { http, seen } = recording(() => ({ data: { id: 1 } }));
	const local = createDataApi(http, 'tmgr.estimate', 'tmgr.estimate', 'Estimate', false, -42);
	await local.createStatus({ name: 'Blocked', type: 'active' });
	await local.updateStatus(3, { name: 'Blocked v2' });
	await local.reorderStatuses([3, 1, 2]);
	await local.createCategory({ title: 'Backend' });
	await local.updateCategory(5, { title: 'Backend team' });
	expect(seen).toEqual([
		'POST workspaces/-42/statuses tmgr.estimate {"name":"Blocked","type":"active"}',
		'PUT statuses/3 tmgr.estimate {"name":"Blocked v2"}',
		'PUT workspaces/-42/statuses/order tmgr.estimate {"statuses_with_order":[{"status_id":3,"order":1},{"status_id":1,"order":2},{"status_id":2,"order":3}]}',
		'POST project_categories tmgr.estimate {"title":"Backend"}',
		'PUT project_categories/5 tmgr.estimate {"title":"Backend team"}',
	]);

	const { http: cloudHttp, seen: cloudSeen } = recording(() => ({ data: { id: 1 } }));
	const cloud = createDataApi(cloudHttp, 'acme.board', 'acme.board', 'Board', true, 99);
	await cloud.createCategory({ title: 'Backend' });
	expect(cloudSeen[0]).toBe(
		'POST project_categories acme.board {"title":"Backend","workspace_id":99}',
	);
});

it('refuses changing a status type or category code in shared workspaces (Java ignores both there)', async () => {
	const { http, seen } = recording(() => ({ data: { id: 1 } }));
	const cloud = createDataApi(http, 'acme.board', 'acme.board', 'Board', true, 99);
	await expect(cloud.updateStatus(3, { type: 'archived' })).rejects.toMatchObject({
		code: 'NOT_SUPPORTED',
	});
	await expect(cloud.updateCategory(5, { code: 'NEW' })).rejects.toMatchObject({
		code: 'NOT_SUPPORTED',
	});
	expect(seen).toEqual([]);
	await cloud.updateStatus(3, { name: 'ok' });
	await cloud.updateCategory(5, { title: 'ok' });
	expect(seen).toHaveLength(2);
});

it('maps agent work calls to the Java routes with snake_case bodies, agent namespaced under the plugin', async () => {
	const { http, seen } = recording(() => ({ data: { id: 9, status: 'running' } }));
	const api = createDataApi(http, 'tmgr.estimate');
	await api.listAgentWork(4);
	await api.startAgentWork(4, {
		agent: 'claude-code',
		model: 'opus',
		sessionId: 'sess-1',
		branch: 'feat/x',
	});
	await api.updateAgentWork(9, { branch: 'feat/y', summary: 'wip' });
	await api.finishAgentWork(9, { status: 'succeeded', summary: 'done' });
	expect(seen).toEqual([
		'GET tasks/4/agent-work tmgr.estimate',
		'POST tasks/4/agent-work tmgr.estimate {"agent":"plugin:tmgr.estimate/claude-code","model":"opus","session_id":"sess-1","branch":"feat/x"}',
		'PATCH agent-work/9 tmgr.estimate {"branch":"feat/y","summary":"wip"}',
		'POST agent-work/9/finish tmgr.estimate {"status":"succeeded","summary":"done"}',
	]);
});

it('namespaces agent work as bare plugin identity when no agent label is given', async () => {
	const { http, seen } = recording(() => ({ data: { id: 9 } }));
	const api = createDataApi(http, 'acme.board');
	await api.startAgentWork(4, { model: null, sessionId: null, branch: null });
	expect(seen[0]).toBe(
		'POST tasks/4/agent-work acme.board {"agent":"plugin:acme.board","model":null,"session_id":null,"branch":null}',
	);
});

describe('pages (API 1.5)', () => {
	const failing = (status: number, data: unknown) =>
		axios.create({
			adapter: async (config) => {
				const error: any = new Error(`status ${status}`);
				error.response = { status, data, config };
				throw error;
			},
		});

	it('maps page calls to the pages REST API as the plugin', async () => {
		const { http, seen } = recording(() => ({ data: { id: 5 } }));
		const api = createDataApi(http, 'acme.dossier');
		await api.pagesSearch('saha', { type: 'person', limit: 5 });
		await api.pagesTree();
		await api.pagesGet('saha');
		await api.pagesCreate({ title: 'T', parent_id: 1 });
		await api.pagesUpdate(5, { version: 2, title: 'N' });
		await api.pagesAppend(5, { markdown: 'm', create_heading: true });
		await api.pagesSetSection(5, 'notes', 'text', 'why');
		await api.pagesSetSection(5, 'fresh', 'text', undefined, 'Fresh');
		expect(seen).toEqual([
			'GET pages/search {"q":"saha","type":"person","limit":5} acme.dossier',
			'GET pages/tree acme.dossier',
			'GET pages/saha acme.dossier',
			'POST pages acme.dossier {"title":"T","parent_id":1}',
			'PATCH pages/5 acme.dossier {"version":2,"title":"N"}',
			'POST pages/5/append acme.dossier {"markdown":"m","create_heading":true}',
			'PUT pages/5/sections/notes acme.dossier {"markdown":"text","summary":"why"}',
			'PUT pages/5/sections/fresh acme.dossier {"markdown":"text","heading":"Fresh"}',
		]);
	});

	it('turns a 409 page_conflict into a typed error carrying the current page', async () => {
		const api = createDataApi(failing(409, { error: 'page_conflict', data: { id: 5, version: 9 } }), 'acme.dossier');
		await expect(api.pagesUpdate(5, { version: 1 })).rejects.toMatchObject({
			code: 'page_conflict',
			data: { id: 5, version: 9 },
		});
	});

	it('maps section_forbidden to PERMISSION_DENIED and a too large page to INVALID_PARAMS', async () => {
		await expect(
			createDataApi(failing(403, { error: 'section_forbidden', message: 'not yours' }), 'a.b').pagesSetSection(1, 's', 'x'),
		).rejects.toMatchObject({ code: 'PERMISSION_DENIED' });
		await expect(
			createDataApi(failing(413, { error: 'page_too_large' }), 'a.b').pagesAppend(1, { markdown: 'x' }),
		).rejects.toMatchObject({ code: 'INVALID_PARAMS' });
	});

	it('stores per-page data under the plugin storage id, and refuses it in shared workspaces', async () => {
		const { http, seen } = recording((_m, url) => (url.includes('/data/') ? { data: { value: '1' } } : { data: {} }));
		const api = createDataApi(http, 'acme.dossier', 'acme.dossier@repo');
		expect(await api.pageDataGet(5, 'seen')).toBe('1');
		await api.pageDataSet(5, 'seen', '2');
		await api.pageDataDelete(5, 'seen');
		await api.pageDataGetMany([5, 6], 'seen');
		expect(seen).toEqual([
			'GET plugins/acme.dossier%40repo/pages/5/data/seen acme.dossier',
			'PUT plugins/acme.dossier%40repo/pages/5/data/seen acme.dossier {"value":"2"}',
			'DELETE plugins/acme.dossier%40repo/pages/5/data/seen acme.dossier',
			'POST plugins/acme.dossier%40repo/page-data/query acme.dossier {"page_ids":[5,6],"key":"seen"}',
		]);
		const shared = createDataApi(http, 'acme.dossier', 'acme.dossier', 'x', true, 3);
		await expect(shared.pageDataGet(5, 'seen')).rejects.toMatchObject({ code: 'NOT_SUPPORTED' });
	});
});
