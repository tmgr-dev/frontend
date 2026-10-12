import { updateUserSettingsV2 } from '@/actions/tmgr/user';
import { syncActiveLocalWorkspace } from '@/utils/localWorkspaceSync';
import { requestCache } from '@/utils/requestCache';

interface StoreLike {
	state: {
		user?: { settings?: { id: number; key: string; value: unknown }[] };
	};
	dispatch(type: string, payload?: unknown): Promise<any>;
	commit(type: string, payload?: unknown): void;
}

interface RouterLike {
	replace(location: string): Promise<unknown> | unknown;
}

/** Switches the app to a cloud workspace the same way the workspace switcher does. */
export const openCloudWorkspace = async (
	store: StoreLike,
	router: RouterLike,
	workspaceId: number,
): Promise<void> => {
	requestCache.invalidate('workspaces');
	const workspaces: { id: number; code: string }[] =
		(await store.dispatch('loadWorkspaces')) ?? [];
	const workspace = workspaces.find(
		(w) => Number(w.id) === Number(workspaceId),
	);
	if (!workspace) {
		throw new Error('The cloud workspace is not in your account list yet.');
	}
	const settings = store.state.user?.settings?.map((setting) => ({
		id: setting.id,
		value: setting.key === 'current_workspace' ? workspace.id : setting.value,
	}));
	store.commit('setUser', await updateUserSettingsV2(settings as any));
	await syncActiveLocalWorkspace(workspace.id);
	store.commit('updateUserWorkspaceSetting', { workspaceId: workspace.id });
	await store.dispatch('featureToggles/loadWorkspaceToggles', workspace.id);
	await router.replace(`/${workspace.code}/list`);
	store.commit('rerenderApp');
};
