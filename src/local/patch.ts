import { LOCAL_CODE_PREFIX, type LocalWorkspace } from './types';

export const CURRENT_WORKSPACE = 'current_workspace';

export const workspaceJson = (workspace: LocalWorkspace, userId: number | null) => ({
	id: workspace.id,
	name: workspace.name,
	code: `${LOCAL_CODE_PREFIX}${workspace.code}`,
	type: 'local',
	is_default: false,
	default_workspace: false,
	user_id: userId,
	is_local: true,
	path: workspace.path,
});

/** The server's workspace list with the local ones appended, each shaped like a cloud workspace. */
export const mergeWorkspaces = (
	cloud: any[],
	local: LocalWorkspace[],
	userId: number | null,
) => [
	...(Array.isArray(cloud) ? cloud.filter((w) => !w?.is_local) : []),
	...local.map((w) => workspaceJson(w, userId)),
];

/** The user as the app should see it: current_workspace points at the active local workspace. */
export const withCurrentWorkspace = (user: any, workspaceId: number) => {
	if (!user || !Array.isArray(user.settings)) return user;
	let found = false;
	const settings = user.settings.map((setting: any) => {
		if (setting?.key !== CURRENT_WORKSPACE) return setting;
		found = true;
		return { ...setting, value: workspaceId };
	});
	if (!found) settings.push({ key: CURRENT_WORKSPACE, value: workspaceId });
	return { ...user, settings };
};

/**
 * The workspace a `PUT v2/user/settings` switches to. The payload lists every setting as
 * `{id, value}`; the current-workspace one is recognised by the id it has in the user's settings.
 */
export const requestedWorkspace = (payload: any, user: any): number | null => {
	const setting = user?.settings?.find((s: any) => s?.key === CURRENT_WORKSPACE);
	if (!setting || !Array.isArray(payload)) return null;
	const entry = payload.find((item: any) => item?.id === setting.id);
	return entry ? Number(entry.value) : null;
};

/** The same payload with current_workspace left at the server's value, so a local id never reaches it. */
export const withoutLocalWorkspace = (payload: any[], user: any, serverWorkspaceId: number | null) => {
	const setting = user?.settings?.find((s: any) => s?.key === CURRENT_WORKSPACE);
	return payload.map((item: any) =>
		setting && item?.id === setting.id ? { ...item, value: serverWorkspaceId } : item,
	);
};
