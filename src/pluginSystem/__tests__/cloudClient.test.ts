import type { AxiosAdapter, InternalAxiosRequestConfig } from 'axios';
import { cloudPluginClient } from '../cloudClient';

const recorder = (
	status: (config: InternalAxiosRequestConfig) => number = () => 200,
	body: (config: InternalAxiosRequestConfig) => unknown = () => ({ data: [] }),
) => {
	const seen: InternalAxiosRequestConfig[] = [];
	const adapter: AxiosAdapter = async (config) => {
		seen.push(config);
		const code = status(config);
		const response = {
			data: body(config),
			status: code,
			statusText: '',
			headers: {},
			config,
		};
		if (code >= 400)
			throw Object.assign(new Error(`status ${code}`), { config, response });
		return response;
	};
	return { adapter, seen };
};

it('sends only the plugin token and names the workspace on lists and creates', async () => {
	const { adapter, seen } = recorder();
	let minted = 0;
	const client = cloudPluginClient({
		baseURL: 'https://api.example.com/api/',
		workspaceId: 56,
		mint: async () => ({ token: `plugin-${++minted}`, expires_in: 900 }),
		adapter,
	});
	await Promise.all([
		client.get('tasks', { params: { page: 1 } }),
		client.get('workspaces/statuses'),
		client.post('tasks', { title: 'x', workspace_id: 9 }),
		client.patch('tasks/5', { title: 'y' }),
	]);
	expect(minted).toBe(1);
	expect(seen.map((c) => c.headers.Authorization)).toEqual(
		Array(4).fill('Bearer plugin-1'),
	);
	expect(seen.map((c) => c.headers['X-Workspace-Id'])).toEqual(
		Array(4).fill('56'),
	);
	expect(seen[0].params).toEqual({ page: 1, workspace_id: 56 });
	expect(seen[1].params).toEqual({ workspace_id: 56 });
	expect(JSON.parse(seen[2].data)).toEqual({ title: 'x', workspace_id: 56 });
	expect(JSON.parse(seen[3].data)).toEqual({ title: 'y' });
});

it('mints a new token before the old one expires and once after a 401', async () => {
	let time = 0;
	let minted = 0;
	let reject = false;
	const { adapter, seen } = recorder((config) =>
		reject && config.headers.Authorization === 'Bearer plugin-2' ? 401 : 200,
	);
	const client = cloudPluginClient({
		baseURL: 'https://api.example.com/api/',
		workspaceId: 56,
		mint: async () => ({ token: `plugin-${++minted}`, expires_in: 900 }),
		now: () => time,
		adapter,
	});
	await client.get('tasks/1');
	time = 850_000;
	await client.get('tasks/1');
	expect(minted).toBe(2);
	reject = true;
	await client.get('tasks/1');
	expect(minted).toBe(3);
	expect(seen[seen.length - 1].headers.Authorization).toBe('Bearer plugin-3');
	await expect(
		cloudPluginClient({
			baseURL: 'https://api.example.com/api/',
			workspaceId: 56,
			mint: async () => ({ token: 'plugin-2', expires_in: 900 }),
			adapter,
		}).get('tasks/1'),
	).rejects.toThrow('status 401');
});

describe('workspace check on responses', () => {
	const clientFor = (
		body: (config: InternalAxiosRequestConfig) => unknown,
		status?: (config: InternalAxiosRequestConfig) => number,
		mint: () => Promise<{ token: string; expires_in: number }> = async () => ({
			token: 'plugin-1',
			expires_in: 900,
		}),
	) =>
		cloudPluginClient({
			baseURL: 'https://api.example.com/api/',
			workspaceId: 56,
			mint,
			adapter: recorder(status, body).adapter,
		});

	it('rejects a list holding a task of another workspace', async () => {
		const client = clientFor(() => ({
			data: [
				{ id: 1, workspace_id: 56 },
				{ id: 2, workspace_id: 9 },
			],
		}));
		await expect(client.get('tasks')).rejects.toMatchObject({
			code: 'PERMISSION_DENIED',
		});
	});

	it('rejects a paginated payload with a foreign task', async () => {
		const client = clientFor(() => ({
			data: { data: [{ id: 2, workspace_id: 9 }] },
		}));
		await expect(client.get('tasks')).rejects.toMatchObject({
			code: 'PERMISSION_DENIED',
		});
	});

	it('rejects a single foreign task and category', async () => {
		await expect(
			clientFor(() => ({ data: { id: 2, workspace_id: 9 } })).get('tasks/2'),
		).rejects.toMatchObject({ code: 'PERMISSION_DENIED' });
		await expect(
			clientFor(() => ({ data: [{ id: 3, workspace_id: '9' }] })).get(
				'project_categories',
			),
		).rejects.toMatchObject({ code: 'PERMISSION_DENIED' });
	});

	it('checks the routes that return a task after a countdown or a conversion', async () => {
		const foreign = () => ({ data: { id: 2, workspace_id: 9 } });
		for (const [method, route] of [
			['post', 'tasks/2/countdown'],
			['delete', 'tasks/2/countdown'],
			['post', 'daily-routines/tasks/2/convert'],
		] as const) {
			await expect(clientFor(foreign)[method](route)).rejects.toMatchObject({
				code: 'PERMISSION_DENIED',
			});
		}
	});

	it('does not check files and agent work, which keep the workspace they were created in', async () => {
		const stale = () => ({ data: [{ id: 4, workspace_id: 9 }] });
		for (const route of ['tasks/2/files', 'tasks/2/agent-work', 'files/4']) {
			await expect(clientFor(stale).get(route)).resolves.toMatchObject({
				status: 200,
			});
		}
	});

	it('passes matching, string and null workspace ids', async () => {
		const client = clientFor(() => ({
			data: [
				{ id: 1, workspace_id: 56 },
				{ id: 2, workspace_id: '56' },
				{ id: 3, workspace_id: null },
				{ id: 4 },
			],
		}));
		await expect(client.get('tasks')).resolves.toMatchObject({ status: 200 });
	});

	it('passes a nested linked task of another workspace', async () => {
		const client = clientFor(() => ({
			data: {
				id: 1,
				workspace_id: 56,
				linked: [{ id: 7, workspace_id: 9 }],
			},
		}));
		await expect(client.get('tasks/1')).resolves.toMatchObject({
			status: 200,
		});
	});

	it('checks the retried response after a 401 and renews once', async () => {
		let minted = 0;
		let first = true;
		const client = clientFor(
			() => ({ data: { id: 2, workspace_id: 9 } }),
			() => {
				if (!first) return 200;
				first = false;
				return 401;
			},
			async () => ({ token: `plugin-${++minted}`, expires_in: 900 }),
		);
		await expect(client.get('tasks/2')).rejects.toMatchObject({
			code: 'PERMISSION_DENIED',
		});
		expect(minted).toBe(2);
	});

	it('carries no response or data in the error', async () => {
		const client = clientFor(() => ({
			data: { id: 2, workspace_id: 9, title: 'secret' },
		}));
		const error: any = await client.get('tasks/2').catch((e) => e);
		expect(error.response).toBeUndefined();
		expect(error.data).toBeUndefined();
		expect(JSON.stringify(error)).not.toContain('secret');
		expect(error.message).not.toContain('secret');
	});
});
