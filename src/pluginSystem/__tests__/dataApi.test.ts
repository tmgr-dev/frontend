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
		items: [{ id: 1 }],
		total: 7,
	});
	expect(await api.updateTask(4, { title: 'x' })).toEqual({ id: 9 });
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

it('maps cloud comments (userId only) to a user author, and passes one through if present', async () => {
	const { http } = recording((method, url) =>
		url === 'tasks/4/comments' && method === 'get'
			? { data: [{ id: 1, userId: 9, message: 'hi' }, { id: 2, author: { kind: 'plugin', id: 'x', name: 'X' }, message: 'yo' }] }
			: { data: { id: 3, userId: 9, message: 'new' } },
	);
	const api = createDataApi(http, 'tmgr.estimate');
	expect(await api.listComments(4)).toEqual([
		{ id: 1, userId: 9, message: 'hi', author: { kind: 'user', id: '9', name: '' } },
		{ id: 2, author: { kind: 'plugin', id: 'x', name: 'X' }, message: 'yo' },
	]);
	expect(await api.addComment(4, 'new')).toEqual({
		id: 3,
		userId: 9,
		message: 'new',
		author: { kind: 'user', id: '9', name: '' },
	});
});

it('resolves a relation type name to its id, caching the lookup per instance', async () => {
	let typeCalls = 0;
	const { http, seen } = recording((_method, url) => {
		if (url === 'task-relation-types') {
			typeCalls++;
			return { data: [{ id: 1, name: 'blocks' }, { id: 3, name: 'relates to' }] };
		}
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
	expect(typeCalls).toBe(1);
	expect(seen.filter((line) => line.startsWith('POST tasks/4/related-to/9/with/1'))).toHaveLength(1);
	expect(seen.filter((line) => line.startsWith('DELETE tasks/4/related-to/9/with/3'))).toHaveLength(1);
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
