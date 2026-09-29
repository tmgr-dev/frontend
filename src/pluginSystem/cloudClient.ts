import axios, { type AxiosAdapter, type AxiosInstance } from 'axios';

export interface MintedToken {
	token: string;
	expires_in: number;
}

const WORKSPACE_LISTS = new Set(['tasks', 'workspaces/statuses', 'project_categories']);
const RENEW_BEFORE_MS = 60_000;

const routeOf = (url?: string) => (url ?? '').replace(/^\/+/, '').split('?')[0];

const isPlainObject = (value: unknown): value is Record<string, unknown> =>
	!!value && typeof value === 'object' && !Array.isArray(value);

/**
 * Plugin data in a shared workspace. A fresh client, never the app's: it carries only the plugin's
 * short-lived token (the server limits it to this workspace and the plugin's permissions), and that token
 * never leaves this module. Every list and create names the workspace, so a workspace switch in the app
 * cannot redirect it.
 */
export const cloudPluginClient = ({
	baseURL,
	workspaceId,
	mint,
	now = Date.now,
	adapter,
}: {
	baseURL: string;
	workspaceId: number;
	mint: () => Promise<MintedToken>;
	now?: () => number;
	adapter?: AxiosAdapter;
}): AxiosInstance => {
	const client = axios.create({ baseURL, timeout: 30_000, adapter });
	let current: { token: string; expiresAt: number } | null = null;
	let minting: Promise<string> | null = null;

	const token = (renew: boolean) => {
		if (!renew && current && current.expiresAt - now() > RENEW_BEFORE_MS)
			return Promise.resolve(current.token);
		minting ??= mint()
			.then(({ token: value, expires_in }) => {
				current = { token: value, expiresAt: now() + expires_in * 1000 };
				return value;
			})
			.finally(() => {
				minting = null;
			});
		return minting;
	};

	client.interceptors.request.use(async (config) => {
		const route = routeOf(config.url);
		const method = (config.method ?? 'get').toLowerCase();
		if (method === 'get' && WORKSPACE_LISTS.has(route)) {
			config.params = { ...config.params, workspace_id: workspaceId };
		}
		if (method === 'post' && route === 'tasks' && isPlainObject(config.data)) {
			config.data = { ...config.data, workspace_id: workspaceId };
		}
		const renew = Boolean((config as { _renewed?: boolean })._renewed);
		config.headers.set('Authorization', `Bearer ${await token(renew)}`);
		// Pinned to this client's own workspace, never the app tab's — that is the point of a
		// dedicated client (see the module comment).
		config.headers.set('X-Workspace-Id', String(workspaceId));
		return config;
	});

	client.interceptors.response.use(undefined, async (error) => {
		const config = error?.config;
		if (error?.response?.status === 401 && config && !config._renewed) {
			config._renewed = true;
			return client.request(config);
		}
		throw error;
	});
	return client;
};
