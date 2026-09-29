import axios, { type AxiosResponse, type InternalAxiosRequestConfig } from 'axios';

const LOCAL = {
	id: -42,
	name: 'Current project',
	code: 'current-project',
	schema_version: 1,
	created_at: '',
	path: '/tmp/current-project',
	database: '/tmp/current-project/workspace.db',
};

jest.mock('@tauri-apps/api/core', () => ({
	invoke: async (command: string) => (command === 'local_workspaces_list' ? [LOCAL] : null),
}));

const storage = new Map<string, string>();
(globalThis as any).localStorage = {
	getItem: (key: string) => storage.get(key) ?? null,
	setItem: (key: string, value: string) => storage.set(key, value),
	removeItem: (key: string) => storage.delete(key),
	clear: () => storage.clear(),
};

type Runtime = typeof import('../runtime');

const openWindow = () => {
	let runtime!: Runtime;
	let install!: typeof import('../install');
	jest.isolateModules(() => {
		runtime = require('../runtime');
		install = require('../install');
	});
	const sent: string[] = [];
	const server = async (config: InternalAxiosRequestConfig): Promise<AxiosResponse> => {
		sent.push(`${config.method} ${config.url}`);
		return { data: '{"data":{"id":1}}', status: 201, statusText: 'Created', headers: {}, config };
	};
	const client = axios.create({ adapter: server });
	install.installLocalWorkspaces(client, { currentUser: () => ({ id: 7 }), hasSession: () => true });
	return { runtime, client, sent };
};

describe('a window opened while the app has a local workspace open', () => {
	beforeEach(() => {
		storage.clear();
		storage.set('local.activeWorkspace', LOCAL.code);
	});

	it('refuses a routine for a cloud workspace until the window stops following the local one', async () => {
		const { runtime, client, sent } = openWindow();
		await runtime.listLocalWorkspaces();

		const error = await client
			.post('daily-routines/tasks', { title: 'asdasd', workspace_id: 1, is_daily_routine: true })
			.catch((e) => e);
		expect(error.response.status).toBe(409);
		expect(error.response.data.message).toBe(
			'This change belongs to another workspace and was not saved; reopen the task',
		);
		expect(sent).toEqual([]);

		runtime.followActiveLocalWorkspace(false);
		await client.post('daily-routines/tasks', { title: 'asdasd', workspace_id: 1, is_daily_routine: true });
		await client.post('/files/presign-upload', { name: 'screenshot.png' });
		await client.post('/tasks/9000/files', { name: 'screenshot.png' });

		expect(sent).toEqual([
			'post daily-routines/tasks',
			'post /files/presign-upload',
			'post /tasks/9000/files',
		]);
		expect(storage.get('local.activeWorkspace')).toBe(LOCAL.code);
	});

	it('picks up the workspace the app switched to after the window was opened', async () => {
		const { runtime } = openWindow();
		await runtime.listLocalWorkspaces();
		expect(runtime.activeLocalWorkspace()).toEqual(LOCAL);

		storage.delete('local.activeWorkspace');
		runtime.followActiveLocalWorkspace();
		expect(runtime.hasActiveLocalWorkspace()).toBe(false);

		storage.set('local.activeWorkspace', LOCAL.code);
		runtime.followActiveLocalWorkspace();
		expect(runtime.activeLocalWorkspace()).toEqual(LOCAL);
	});
});
