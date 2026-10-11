import axios, { AxiosError, type AxiosResponse, type InternalAxiosRequestConfig } from 'axios';

import { migrate } from '../schema';
import type { LocalWorkspace } from '../types';
import { memoryDb, nodeSqliteAvailable } from './nodeDb';

const LOCAL: LocalWorkspace = {
	id: -42,
	name: 'My workspace',
	code: 'my-workspace',
	schema_version: 1,
	created_at: '',
	path: '/tmp/my-workspace',
	database: '/tmp/my-workspace/workspace.db',
};

const storage = new Map<string, string>();
(globalThis as any).localStorage = {
	getItem: (key: string) => storage.get(key) ?? null,
	setItem: (key: string, value: string) => storage.set(key, value),
	removeItem: (key: string) => storage.delete(key),
	clear: () => storage.clear(),
};

let active: LocalWorkspace | null = null;
let workspaces: LocalWorkspace[] = [LOCAL];
const db = nodeSqliteAvailable ? memoryDb() : null;

jest.mock('../runtime', () => ({
	listLocalWorkspaces: async () => workspaces,
	activeLocalWorkspace: () => active,
	hasActiveLocalWorkspace: () => active !== null,
	setActiveLocalWorkspace: (w: LocalWorkspace | null) => {
		active = w;
	},
	localWorkspaceById: async (id: number) => workspaces.find((w) => w.id === id) ?? null,
	localContext: async (workspace: LocalWorkspace, user: any) => ({
		db,
		workspace,
		user,
		now: () => new Date('2026-10-11T10:00:00Z'),
		files: { url: (key: string) => key, read: async () => new Blob([]), remove: async () => {} },
	}),
}));

// eslint-disable-next-line import/first
import { installLocalWorkspaces } from '../install';
// eslint-disable-next-line import/first
import { ACCOUNT_ROUTES_SAMPLE } from './accountRoutes';

const describeSqlite = nodeSqliteAvailable ? describe : describe.skip;

describeSqlite('installLocalWorkspaces without an account', () => {
	const sent: string[] = [];
	let accountless = true;
	let user: any = {};
	let ready: Promise<unknown> = Promise.resolve();

	const network = async (config: InternalAxiosRequestConfig): Promise<AxiosResponse> => {
		sent.push(`${config.method} ${config.url}`);
		throw new AxiosError('Network Error', AxiosError.ERR_NETWORK, config);
	};

	const client = axios.create({ adapter: network });
	installLocalWorkspaces(client, {
		currentUser: () => user,
		hasSession: () => !accountless,
		accountless: () => accountless,
		ready: () => ready,
		profileName: () => 'Ada',
	});

	beforeAll(async () => {
		await migrate(db!, '2026-10-11T10:00:00Z');
	});

	beforeEach(() => {
		sent.length = 0;
		accountless = true;
		ready = Promise.resolve();
		active = null;
		workspaces = [LOCAL];
		user = {};
		localStorage.clear();
	});

	const attempt = (method: string, url: string, data?: unknown) =>
		client.request({ method, url, data }).catch((error) => error);

	it('never lets any request reach the network, with or without an active local workspace', async () => {
		const arbitrary: [string, string][] = [
			['GET', 'dashboard'],
			['GET', 'pomodoro/status'],
			['POST', 'pomodoro/start'],
			['GET', 'tasks/1'],
			['GET', 'tasks/current'],
			['POST', 'tasks'],
			['GET', 'tasks/runned'],
			['GET', 'workspaces/5/modules'],
			['GET', 'workspaces/statuses'],
			['DELETE', 'tasks/900/countdown'],
		];
		for (const withWorkspace of [false, true]) {
			active = withWorkspace ? LOCAL : null;
			for (const [method, url] of [...ACCOUNT_ROUTES_SAMPLE, ...arbitrary]) {
				await attempt(method, url);
			}
		}
		expect(sent).toEqual([]);
	});

	it('refuses an account route with requires_account', async () => {
		for (const [method, url] of [
			['POST', 'notify-tokens'],
			['GET', 'persona-tokens'],
			['GET', 'stats'],
			['GET', 'broadcasting/auth'],
			['POST', 'telegram/link/generate'],
			['POST', 'personas'],
		]) {
			const error = await attempt(method, url);
			expect(error.response.status).toBe(403);
			expect(error.response.data).toEqual({
				message: 'This needs a TMGR account',
				code: 'requires_account',
			});
		}
	});

	it('lets sign-in calls through to the server', async () => {
		await attempt('POST', 'auth/login');
		await attempt('POST', 'auth/register');
		await attempt('POST', 'auth/login/desktop/accept');
		await attempt('POST', 'password/email');
		expect(sent).toEqual([
			'post auth/login',
			'post auth/register',
			'post auth/login/desktop/accept',
			'post password/email',
		]);
	});

	it('answers GET user with the local identity and activates the first local workspace', async () => {
		const { data } = await client.get('user');

		expect(data.data).toMatchObject({ id: -1, name: 'Ada', email: '', is_local: true });
		const current = data.data.settings.find((s: any) => s.key === 'current_workspace');
		expect(current.value).toBe(LOCAL.id);
		expect(typeof current.id).toBe('number');
		expect(active).toEqual(LOCAL);
	});

	it('keeps the already active local workspace as current', async () => {
		const other = { ...LOCAL, id: -43, code: 'other', name: 'Other' };
		workspaces = [LOCAL, other];
		active = other;

		const { data } = await client.get('user');

		expect(data.data.settings.find((s: any) => s.key === 'current_workspace').value).toBe(-43);
	});

	it('answers GET workspaces with the local workspaces only', async () => {
		const { data } = await client.get('workspaces');

		expect(data.data.map((w: any) => w.code)).toEqual(['local-my-workspace']);
		expect(sent).toEqual([]);
	});

	it('switches workspace through v2/user/settings without a cloud workspace', async () => {
		const other = { ...LOCAL, id: -43, code: 'other', name: 'Other' };
		workspaces = [LOCAL, other];
		const { data: me } = await client.get('user');
		const id = me.data.settings.find((s: any) => s.key === 'current_workspace').id;

		const { data } = await client.put('v2/user/settings', [{ id, value: -43 }]);

		expect(active).toEqual(other);
		expect(data.data.settings.find((s: any) => s.key === 'current_workspace').value).toBe(-43);
		expect(sent).toEqual([]);
	});

	it('refuses a switch to a workspace that does not exist', async () => {
		const { data: me } = await client.get('user');
		const id = me.data.settings.find((s: any) => s.key === 'current_workspace').id;

		const error = await attempt('PUT', 'v2/user/settings', [{ id, value: -999 }]);

		expect(error.response.status).toBe(404);
	});

	it('keeps theme settings locally', async () => {
		const { data: me } = await client.get('user');
		const theme = me.data.settings.find((s: any) => s.key === 'theme');

		await client.put('v2/user/settings', [{ id: theme.id, value: 'ocean' }]);

		const list = (await client.get('v2/user/settings')).data.data;
		expect(list.find((s: any) => s.key === 'theme').value).toBe('ocean');
		expect((await client.get('user')).data.data.settings.find((s: any) => s.key === 'theme').value).toBe('ocean');
	});

	it('keeps user feature toggles locally', async () => {
		const first = (await client.get('user/feature-toggles')).data.data;
		expect(first.default_landing_page.value).toBe('list');

		await client.put('user/feature-toggles', { features: { default_landing_page: 'board' } });

		expect((await client.get('user/feature-toggles')).data.data.default_landing_page.value).toBe('board');
		expect(sent).toEqual([]);
	});

	it('keeps legacy user settings locally', async () => {
		await client.put('user/settings', { showTooltips: false });

		const { data } = await client.get('user/settings');

		expect(data.data.settings.showTooltips).toBe(false);
	});

	it('answers notifications and personas with empty lists', async () => {
		expect((await client.get('notifications')).data.data).toEqual([]);
		expect((await client.get('notifications/unread-count')).data.count).toBe(0);
		expect((await client.get('personas')).data.data).toEqual([]);
	});

	it('refuses workspace data locally while no local workspace is active', async () => {
		const error = await attempt('GET', 'tasks/current');

		expect(error.response.status).toBe(409);
		expect(sent).toEqual([]);
	});

	it('lists running timers from the local workspace only', async () => {
		active = LOCAL;

		const { data } = await client.get('tasks/runned');

		expect(Array.isArray(data.data)).toBe(true);
		expect(sent).toEqual([]);
	});

	it('refuses a timer call pinned to a cloud workspace', async () => {
		active = LOCAL;

		const error = await client.delete('tasks/900/countdown', { params: { workspace_id: 56 } }).catch((e) => e);

		expect(error.response.status).toBe(403);
		expect(sent).toEqual([]);
	});

	it('writes with the local identity', async () => {
		active = LOCAL;

		const { data } = await client.post('tasks', { title: 'Mine' });

		expect(data.data.user_id).toBe(-1);
		expect(data.data.user.name).toBe('Ada');
	});

	it('holds every request until the profile is read, so none leaves as a signed-in one', async () => {
		accountless = false;
		let resolve!: () => void;
		ready = new Promise<void>((done) => {
			resolve = done;
		});

		const pending = Promise.all([attempt('GET', 'tasks/current'), attempt('GET', 'notifications/unread-count')]);
		await Promise.resolve();
		accountless = true;
		resolve();
		await pending;

		expect(sent).toEqual([]);
	});

	it('goes back to the server once an account is signed in', async () => {
		accountless = false;
		user = { id: 7, name: 'Yurij', settings: [{ id: 5, key: 'current_workspace', value: 56 }] };

		await attempt('GET', 'dashboard');
		await attempt('GET', 'notifications/unread-count');

		expect(sent).toEqual(['get dashboard', 'get notifications/unread-count']);
	});
});
