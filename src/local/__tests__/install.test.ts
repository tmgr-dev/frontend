import axios, { AxiosError, type AxiosResponse, type InternalAxiosRequestConfig } from 'axios';

import { migrate } from '../schema';
import type { LocalWorkspace } from '../types';
import { memoryDb, nodeSqliteAvailable } from './nodeDb';
import { createDomainEvents, installDomainEvents, type DomainEvent } from '@/utils/domainEvents';

const LOCAL: LocalWorkspace = {
	id: -42,
	name: 'Personal',
	code: 'personal',
	schema_version: 1,
	created_at: '',
	path: '/tmp/personal',
	database: '/tmp/personal/workspace.db',
};

const storage = new Map<string, string>();
(globalThis as any).localStorage = {
	getItem: (key: string) => storage.get(key) ?? null,
	setItem: (key: string, value: string) => storage.set(key, value),
	removeItem: (key: string) => storage.delete(key),
	clear: () => storage.clear(),
};

let active: LocalWorkspace | null = null;
const db = nodeSqliteAvailable ? memoryDb() : null;

jest.mock('../runtime', () => ({
	listLocalWorkspaces: async () => [LOCAL],
	activeLocalWorkspace: () => active,
	hasActiveLocalWorkspace: () => active !== null,
	setActiveLocalWorkspace: (w: LocalWorkspace | null) => {
		active = w;
	},
	localWorkspaceById: async (id: number) => (id === LOCAL.id ? LOCAL : null),
	localContext: async (workspace: LocalWorkspace, user: any) => ({
		db,
		workspace,
		user,
		now: () => new Date('2026-09-26T10:00:00Z'),
		files: { url: (key: string) => key, read: async () => new Blob([]), remove: async () => {} },
	}),
}));

// eslint-disable-next-line import/first
import { installLocalWorkspaces } from '../install';
// eslint-disable-next-line import/first
import { pinnedLocalClient } from '../pinned';

const describeSqlite = nodeSqliteAvailable ? describe : describe.skip;

describeSqlite('installLocalWorkspaces', () => {
	const sent: { method: string; url: string; data: any }[] = [];
	let offline = false;
	let user: any;

	const server = async (config: InternalAxiosRequestConfig): Promise<AxiosResponse> => {
		if (offline) throw new AxiosError('Network Error', AxiosError.ERR_NETWORK, config);
		sent.push({ method: config.method!, url: config.url!, data: config.data });
		const body =
			config.url === 'workspaces'
				? { data: [{ id: 56, name: 'TMGR.DEV', code: 'tmgrdev' }] }
				: config.url === 'tasks/runned'
				? { data: [{ id: 900, title: 'Cloud timer', workspace_id: 56 }], meta: {} }
				: config.url === 'user' || config.url === 'v2/user/settings'
				? { data: { id: 7, name: 'Yurij', settings: [{ id: 5, key: 'current_workspace', value: 56 }] } }
				: { data: 'from server' };
		return { data: JSON.stringify(body), status: 200, statusText: 'OK', headers: {}, config };
	};

	const client = axios.create({ adapter: server });
	installLocalWorkspaces(client, { currentUser: () => user, hasSession: () => true });
	const events = createDomainEvents();
	installDomainEvents(client, events, () => 56);

	beforeAll(async () => {
		await migrate(db!, '2026-09-26T10:00:00Z');
	});

	beforeEach(() => {
		sent.length = 0;
		offline = false;
		active = null;
		user = { id: 7, name: 'Yurij', settings: [{ id: 5, key: 'current_workspace', value: 56 }] };
		localStorage.clear();
	});

	it('lists local workspaces next to the cloud ones and leaves other calls alone', async () => {
		const { data } = await client.get('workspaces');
		expect(data.data.map((w: any) => w.code)).toEqual(['tmgrdev', 'local-personal']);
		expect((await client.get('tasks/current')).data.data).toBe('from server');
	});

	it('switching to a local workspace keeps the server on its cloud workspace', async () => {
		await client.get('user');
		sent.length = 0;
		const { data } = await client.put('v2/user/settings', [{ id: 5, value: -42 }]);

		expect(JSON.parse(sent[0].data)).toEqual([{ id: 5, value: 56 }]);
		expect(data.data.settings[0].value).toBe(-42);
		expect(active).toEqual(LOCAL);
	});

	it('inside a local workspace answers workspace calls from SQLite and never sends them', async () => {
		active = LOCAL;
		const created = await client.post('tasks', { title: 'Offline task' });
		const list = await client.get('tasks/current', { params: { page: 1 } });

		expect(created.status).toBe(201);
		expect(list.data.data.map((t: any) => t.title)).toContain('Offline task');
		expect(sent).toEqual([]);
	});

	it('reports domain events for local writes with the local workspace id', async () => {
		active = LOCAL;
		const seen: DomainEvent[] = [];
		const off = events.on((event) => seen.push(event));
		const task = (await client.post('tasks', { title: 'Evented' })).data.data;
		await client.post(`tasks/${task.id}/countdown`);
		off();
		await client.delete(`tasks/${task.id}/countdown`);

		expect(seen.map((e) => [e.type, e.workspaceId])).toEqual([
			['task.created', LOCAL.id],
			['timer.started', LOCAL.id],
		]);
		expect(sent).toEqual([]);
	});

	it('a pinned client stays in its local workspace whatever the app switches to', async () => {
		const pinned = pinnedLocalClient(LOCAL.id, () => ({ id: 7, name: 'Yurij', email: '' }));
		active = null;
		const created = await pinned.post('tasks', { title: 'Written by a plugin' });
		expect(created.data.data.workspace_id).toBe(LOCAL.id);
		expect(sent).toEqual([]);

		const gone = pinnedLocalClient(-999, () => ({ id: 7, name: '', email: '' }));
		await expect(gone.get('tasks')).rejects.toMatchObject({ response: { status: 409 } });
		await expect(pinned.post('agent/conversations', {})).rejects.toMatchObject({ response: { status: 501 } });
		expect(sent).toEqual([]);
	});

	it('refuses what the local API does not implement instead of sending it', async () => {
		active = LOCAL;
		const error = await client.post('agent/conversations', { message: 'summarise my tasks' }).catch((e) => e);

		expect(error.response.status).toBe(501);
		expect(sent).toEqual([]);
	});

	it('keeps account calls on the server and shows the local workspace as current', async () => {
		active = LOCAL;
		await client.get('user/feature-toggles');
		const { data } = await client.get('user');

		expect(sent.map((r) => r.url)).toEqual(['user/feature-toggles', 'user']);
		expect(data.data.settings[0].value).toBe(-42);
	});

	it('starts offline inside a local workspace from the last server answers', async () => {
		active = LOCAL;
		await client.get('user');
		offline = true;

		const { data } = await client.get('user');
		expect(data.data.name).toBe('Yurij');
	});

	it('switching back to a cloud workspace leaves local mode', async () => {
		active = LOCAL;
		user = { ...user, settings: [{ id: 5, key: 'current_workspace', value: -42 }] };
		await client.put('v2/user/settings', [{ id: 5, value: 56 }]);

		expect(active).toBeNull();
		expect(JSON.parse(sent[0].data)).toEqual([{ id: 5, value: 56 }]);
	});

	it('refuses a switch to a local workspace before the cloud one is known, never sending null', async () => {
		const error = await client.put('v2/user/settings', [{ id: 5, value: -42 }]).catch((e) => e);

		expect(error.response.status).toBe(409);
		expect(sent).toEqual([]);
		expect(active).toBeNull();
	});

	it('a local autosave that fires after switching back to the cloud is refused, not sent', async () => {
		const error = await client
			.put('tasks/5', { title: 'Private', workspace_id: -42 })
			.catch((e) => e);

		expect(error.response.status).toBe(409);
		expect(sent).toEqual([]);
	});

	it('a cloud autosave that fires after switching to a local workspace does not write into it', async () => {
		active = LOCAL;
		const task = (await client.post('tasks', { title: 'Mine' })).data.data;

		const error = await client
			.put(`tasks/${task.id}`, { title: 'From the cloud', workspace_id: 56 })
			.catch((e) => e);

		expect(error.response.status).toBe(409);
		expect((await client.get(`tasks/${task.id}`)).data.data.title).toBe('Mine');
	});

	it('lists running timers of the local workspace and of the cloud together', async () => {
		active = LOCAL;
		const task = (await client.post('tasks', { title: 'Local timer' })).data.data;
		await client.post(`tasks/${task.id}/countdown`);

		const { data } = await client.get('tasks/runned');

		expect(data.data.map((t: any) => t.title)).toEqual(['Local timer', 'Cloud timer']);
		await client.delete(`tasks/${task.id}/countdown`);
	});

	it('stops a cloud timer from the tray while a local workspace is open', async () => {
		active = LOCAL;
		await client.delete('tasks/900/countdown', { params: { workspace_id: 56 } });

		expect(sent.map((r) => `${r.method} ${r.url}`)).toEqual(['delete tasks/900/countdown']);
	});
});
