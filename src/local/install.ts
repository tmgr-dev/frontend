import axios, {
	AxiosError,
	type AxiosAdapter,
	type AxiosInstance,
	type AxiosResponse,
	type InternalAxiosRequestConfig,
} from 'axios';

import { createLocalApi } from './api';
import { classify } from './classify';
import { dispatchLocal } from './dispatch';
import {
	CURRENT_WORKSPACE,
	mergeWorkspaces,
	requestedWorkspace,
	withCurrentWorkspace,
	withoutLocalWorkspace,
} from './patch';
import {
	activeLocalWorkspace,
	hasActiveLocalWorkspace,
	listLocalWorkspaces,
	localContext,
	localWorkspaceById,
	setActiveLocalWorkspace,
} from './runtime';

interface Hooks {
	currentUser: () => any;
}

const CACHE_PREFIX = 'local.serverCache:';

const remember = (key: string, data: unknown) => {
	try {
		localStorage.setItem(CACHE_PREFIX + key, JSON.stringify(data));
	} catch {
		/* storage full or unavailable: offline start just is not possible then */
	}
};

const recall = (key: string): any => {
	try {
		const raw = localStorage.getItem(CACHE_PREFIX + key);
		return raw ? JSON.parse(raw) : null;
	} catch {
		return null;
	}
};

const respond = (
	config: InternalAxiosRequestConfig,
	status: number,
	data: unknown,
): AxiosResponse => {
	const response: AxiosResponse = {
		data,
		status,
		statusText: String(status),
		headers: {},
		config,
		request: { local: true },
	};
	if (status >= 400) {
		throw new AxiosError(
			(data as any)?.message ?? `Request failed with status code ${status}`,
			status >= 500 ? AxiosError.ERR_BAD_RESPONSE : AxiosError.ERR_BAD_REQUEST,
			config,
			response.request,
			response,
		);
	}
	return response;
};

/** Adapters hand back the raw body; axios parses it only afterwards. */
const parsed = (response: AxiosResponse): AxiosResponse => {
	if (typeof response.data !== 'string') return response;
	try {
		return { ...response, data: JSON.parse(response.data) };
	} catch {
		return response;
	}
};

const serverWorkspaceOf = (user: any): number | null => {
	const value = user?.settings?.find((s: any) => s?.key === CURRENT_WORKSPACE)?.value;
	return value === undefined || value === null ? null : Number(value);
};

/**
 * Routes the desktop app's API calls for local workspaces (see `classify`). Local calls never
 * reach the network: the adapter answers them from SQLite or refuses them with 501.
 */
export const installLocalWorkspaces = (instance: AxiosInstance, hooks: Hooks) => {
	const network = axios.getAdapter(instance.defaults.adapter ?? axios.defaults.adapter);
	const api = createLocalApi();

	/** The server's answer, or — offline inside a local workspace — the last one it gave. */
	const networkOrCache = async (config: InternalAxiosRequestConfig, key: string) => {
		try {
			const response = parsed(await network(config));
			remember(key, response.data);
			return response;
		} catch (error) {
			const cached = recall(key);
			if (cached && hasActiveLocalWorkspace() && !(error as AxiosError).response) {
				return respond(config, 200, cached);
			}
			throw error;
		}
	};

	const workspacesAdapter: AxiosAdapter = async (config) => {
		const [response, local] = await Promise.all([
			networkOrCache(config, 'workspaces'),
			listLocalWorkspaces(),
		]);
		const body = response.data ?? {};
		const merged = mergeWorkspaces(body.data ?? body, local, hooks.currentUser()?.id ?? null);
		return { ...response, data: Array.isArray(body) ? merged : { ...body, data: merged } };
	};

	const userAdapter: AxiosAdapter = async (config) => {
		const response = await networkOrCache(config, 'user');
		const active = activeLocalWorkspace();
		if (!active || !response.data?.data) return response;
		remember('serverWorkspace', serverWorkspaceOf(response.data.data));
		return {
			...response,
			data: { ...response.data, data: withCurrentWorkspace(response.data.data, active.id) },
		};
	};

	const settingsAdapter: AxiosAdapter = async (config) => {
		const payload = typeof config.data === 'string' ? JSON.parse(config.data) : config.data;
		const user = hooks.currentUser();
		const target = requestedWorkspace(payload, user);
		if (target === null || target >= 0) {
			if (target !== null) setActiveLocalWorkspace(null);
			return network(config);
		}
		const workspace = await localWorkspaceById(target);
		if (!workspace) return respond(config, 404, { message: 'Local workspace not found' });
		const serverWorkspace = recall('serverWorkspace') ?? serverWorkspaceOf(user);
		const safe = withoutLocalWorkspace(payload, user, serverWorkspace >= 0 ? serverWorkspace : null);
		const response = parsed(await network({ ...config, data: JSON.stringify(safe) }));
		setActiveLocalWorkspace(workspace);
		return {
			...response,
			data: { ...response.data, data: withCurrentWorkspace(response.data?.data, workspace.id) },
		};
	};

	const localAdapter: AxiosAdapter = async (config) => {
		await listLocalWorkspaces();
		const workspace = activeLocalWorkspace();
		if (!workspace) {
			setActiveLocalWorkspace(null);
			return respond(config, 409, { message: 'The local workspace is gone; pick another workspace' });
		}
		const user = hooks.currentUser() ?? recall('user')?.data ?? {};
		const ctx = await localContext(workspace, {
			id: Number(user.id ?? 0),
			name: user.name ?? '',
			email: user.email ?? '',
		});
		const result = await dispatchLocal(
			api,
			ctx,
			config.method ?? 'get',
			config.url ?? '',
			config.data,
			config.params,
		);
		if (!result) {
			return respond(config, 501, {
				message: `Not available in local workspaces yet: ${(config.method ?? 'get').toUpperCase()} ${config.url}`,
			});
		}
		return respond(config, result.status, result.data);
	};

	instance.interceptors.request.use((config) => {
		const route = classify(config.method ?? 'get', config.url ?? '', hasActiveLocalWorkspace());
		if (route === 'server:workspaces') config.adapter = workspacesAdapter;
		else if (route === 'server:user') config.adapter = userAdapter;
		else if (route === 'settings') config.adapter = settingsAdapter;
		else if (route === 'local') config.adapter = localAdapter;
		return config;
	});
};
