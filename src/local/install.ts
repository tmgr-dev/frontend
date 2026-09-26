import axios, {
	AxiosError,
	type AxiosAdapter,
	type AxiosInstance,
	type AxiosResponse,
	type InternalAxiosRequestConfig,
} from 'axios';

import { createLocalApi } from './api';
import { classify, crossesWorkspaces } from './classify';
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
	hasSession: () => boolean;
}

const CACHE_PREFIX = 'local.serverCache:';
const OWNER_KEY = `${CACHE_PREFIX}owner`;

const storage = {
	get(key: string): any {
		try {
			const raw = localStorage.getItem(key);
			return raw ? JSON.parse(raw) : null;
		} catch {
			return null;
		}
	},
	set(key: string, value: unknown) {
		try {
			localStorage.setItem(key, JSON.stringify(value));
		} catch {
			/* storage full or unavailable: offline start just is not possible then */
		}
	},
};

/** Server answers kept per user, so a later login by someone else never sees them. */
const cacheOwner = (): number | null => storage.get(OWNER_KEY);

const adoptOwner = (userId: number) => {
	if (cacheOwner() === userId) return;
	try {
		Object.keys(localStorage)
			.filter((key) => key.startsWith(CACHE_PREFIX))
			.forEach((key) => localStorage.removeItem(key));
	} catch {
		/* nothing cached to drop */
	}
	storage.set(OWNER_KEY, userId);
};

const remember = (key: string, data: unknown) => {
	const owner = cacheOwner();
	if (owner !== null) storage.set(`${CACHE_PREFIX}${owner}:${key}`, data);
};

const recall = (key: string): any => {
	const owner = cacheOwner();
	return owner === null ? null : storage.get(`${CACHE_PREFIX}${owner}:${key}`);
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
			if (
				cached &&
				hooks.hasSession() &&
				hasActiveLocalWorkspace() &&
				!(error as AxiosError).response
			) {
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
		const [response] = await Promise.all([networkOrCache(config, 'user'), listLocalWorkspaces()]);
		const user = response.data?.data;
		if (user?.id) {
			adoptOwner(Number(user.id));
			remember('user', response.data);
			const serverWorkspace = serverWorkspaceOf(user);
			if (serverWorkspace !== null && serverWorkspace >= 0) remember('serverWorkspace', serverWorkspace);
		}
		const active = activeLocalWorkspace();
		if (!active || !user) return response;
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
		const serverWorkspace = recall('serverWorkspace');
		if (serverWorkspace === null || serverWorkspace < 0) {
			return respond(config, 409, {
				message: 'Open one of your cloud workspaces once before switching to a local one',
			});
		}
		const safe = withoutLocalWorkspace(payload, user, serverWorkspace);
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
		const current = hooks.currentUser();
		const user = current?.id ? current : (recall('user')?.data ?? {});
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

	/** Local running timers first, then the cloud ones; a cloud outage just leaves the local ones. */
	const runnedAdapter: AxiosAdapter = async (config) => {
		const [local, cloud] = await Promise.all([
			localAdapter(config),
			network(config)
				.then((response) => parsed(response).data?.data ?? [])
				.catch(() => []),
		]);
		const rows = [...(local.data?.data ?? []), ...(Array.isArray(cloud) ? cloud : [])];
		return {
			...local,
			data: { ...local.data, data: rows, meta: { ...local.data?.meta, total: rows.length } },
		};
	};

	const refuse: AxiosAdapter = async (config) =>
		respond(config, 409, {
			message: 'This change belongs to another workspace and was not saved; reopen the task',
		});

	instance.interceptors.request.use(async (config) => {
		const localMode = hasActiveLocalWorkspace();
		const route = classify(config.method ?? 'get', config.url ?? '', localMode, config.params);
		if (localMode) await listLocalWorkspaces();
		if (crossesWorkspaces(route, config.data, config.params, activeLocalWorkspace()?.id ?? null)) {
			config.adapter = refuse;
			return config;
		}
		if (route === 'server:workspaces') config.adapter = workspacesAdapter;
		else if (route === 'server:user') config.adapter = userAdapter;
		else if (route === 'settings') config.adapter = settingsAdapter;
		else if (route === 'runned') config.adapter = runnedAdapter;
		else if (route === 'local') config.adapter = localAdapter;
		return config;
	});
};
