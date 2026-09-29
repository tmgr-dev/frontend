import type { AxiosAdapter, InternalAxiosRequestConfig } from 'axios';
import { cloudPluginClient } from '../cloudClient';

const recorder = (status: (config: InternalAxiosRequestConfig) => number = () => 200) => {
	const seen: InternalAxiosRequestConfig[] = [];
	const adapter: AxiosAdapter = async (config) => {
		seen.push(config);
		const code = status(config);
		const response = { data: { data: [] }, status: code, statusText: '', headers: {}, config };
		if (code >= 400) throw Object.assign(new Error(`status ${code}`), { config, response });
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
	expect(seen.map((c) => c.headers['X-Workspace-Id'])).toEqual(Array(4).fill('56'));
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
