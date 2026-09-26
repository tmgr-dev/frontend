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
