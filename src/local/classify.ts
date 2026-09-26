import { normalizePath } from './router';

export type Route =
	| 'server'
	| 'server:workspaces'
	| 'server:user'
	| 'settings'
	| 'local'
	| 'blocked';

/** Account-level endpoints that carry no workspace data: they keep going to the server. */
const ACCOUNT = [
	/^auth(\/|$)/,
	/^user$/,
	/^user\/settings$/,
	/^user\/feature-toggles$/,
	/^user\/avatar/,
	/^notifications(\/|$)/,
	/^notification-settings(\/|$)/,
	/^broadcasting\/auth$/,
	/^error-reports$/,
];

/**
 * Where a request goes. Outside a local workspace everything goes to the server, except that the
 * workspace list and the current-workspace switch learn about local workspaces. Inside one, only
 * account-level calls reach the server; every other call is answered locally or refused — it is
 * never sent, so local data cannot leak through an endpoint the local API does not implement.
 */
export const classify = (method: string, url: string, localActive: boolean): Route => {
	const path = normalizePath(url);
	const verb = method.toUpperCase();
	if (verb === 'GET' && path === 'workspaces') return 'server:workspaces';
	if (verb === 'PUT' && path === 'v2/user/settings') return 'settings';
	if (!localActive) return 'server';
	if (verb === 'GET' && path === 'user') return 'server:user';
	if (ACCOUNT.some((pattern) => pattern.test(path))) return 'server';
	return 'local';
};

export const isLocalWorkspaceId = (id: unknown): boolean => Number(id) < 0;
