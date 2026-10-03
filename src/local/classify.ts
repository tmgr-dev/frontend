import { normalizePath } from './router';

export type Route =
	| 'server'
	| 'server:workspaces'
	| 'server:user'
	| 'settings'
	| 'runned'
	| 'local'
	| 'blocked';

/** Account-level endpoints that carry no workspace data: they keep going to the server. */
const ACCOUNT = [
	/^auth(\/|$)/,
	/^user$/,
	/^user\/settings$/,
	/^user\/feature-toggles$/,
	/^user\/avatar/,
	/^user\/alarm-phone(\/|$)/,
	/^notifications(\/|$)/,
	/^notification-settings(\/|$)/,
	/^broadcasting\/auth$/,
	// Persona catalog lives on the account, not the workspace, even while a local one is active.
	/^personas(\/|$)/,
	/^persona-tokens(\/|$)/,
	/^notify-tokens(\/|$)/,
	/^smart-device\/token(\/|$)/,
	/^telegram\/(link|unlink)(\/|$)/,
	/^user\/store-avatar$/,
	/^password(\/|$)/,
	/^stats$/,
	/^error-reports$/,
	/^v2\/user\/settings$/,
];

/**
 * Where a request goes. Outside a local workspace everything goes to the server, except that the
 * workspace list and the current-workspace switch learn about local workspaces. Inside one, only
 * account-level calls reach the server; every other call is answered locally or refused — it is
 * never sent, so local data cannot leak through an endpoint the local API does not implement.
 */
export const classify = (
	method: string,
	url: string,
	localActive: boolean,
	params?: Record<string, any>,
): Route => {
	const path = normalizePath(url);
	const verb = method.toUpperCase();
	if (verb === 'GET' && path === 'workspaces') return 'server:workspaces';
	if (verb === 'PUT' && path === 'v2/user/settings') return 'settings';
	if (verb === 'GET' && path === 'user') return 'server:user';
	if (!localActive) return 'server';
	if (ACCOUNT.some((pattern) => pattern.test(path))) return 'server';
	// Running timers of every workspace, cloud and local, show together in the tray and status bar.
	if (verb === 'GET' && path === 'tasks/runned') return 'runned';
	// A timer call pinned to a cloud workspace (from the tray) targets that cloud task explicitly.
	const target = params?.workspace_id;
	if (/^tasks\/\d+\/countdown$/.test(path) && target !== undefined && Number(target) >= 0) {
		return 'server';
	}
	return 'local';
};

export const isLocalWorkspaceId = (id: unknown): boolean => Number(id) < 0;

const workspaceIdIn = (value: unknown): number | null => {
	let data = value;
	if (typeof data === 'string') {
		try {
			data = JSON.parse(data);
		} catch {
			return null;
		}
	}
	const id = (data as any)?.workspace_id;
	return id === undefined || id === null || id === '' ? null : Number(id);
};

/**
 * A request whose payload belongs to another workspace than the one it would reach — typically a
 * debounced autosave that fires right after a workspace switch. Refusing it keeps local data off
 * the server and cloud (or other local) data out of the active local workspace.
 */
export const crossesWorkspaces = (
	route: Route,
	body: unknown,
	params: unknown,
	activeLocalId: number | null,
): boolean => {
	const ids = [workspaceIdIn(body), workspaceIdIn(params)].filter((id): id is number => id !== null);
	if (!ids.length) return false;
	if (route === 'local') return ids.some((id) => id !== activeLocalId);
	return ids.some((id) => id < 0);
};
