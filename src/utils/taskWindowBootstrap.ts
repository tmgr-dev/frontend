interface Workspace {
	id: number;
	code: string;
}

interface BootstrapStore {
	state: { workspaces?: Workspace[] | null };
	commit: (mutation: string, payload?: unknown) => void;
}

export interface TaskWindowBootstrapDeps {
	getWorkspaces: () => Promise<Workspace[]>;
	syncActiveLocalWorkspace: (workspaceId: number) => Promise<void>;
	getUserFeatureToggles: () => Promise<Record<string, unknown>>;
	getWorkspaceFeatureToggles: (
		workspaceId: number,
	) => Promise<Record<string, unknown>>;
}

export const bootstrapWindowWorkspace = async (
	store: BootstrapStore,
	deps: TaskWindowBootstrapDeps,
	workspaceCode: string,
): Promise<Workspace | null> => {
	if (!store.state.workspaces?.length) {
		store.commit('setWorkspaces', await deps.getWorkspaces());
	}
	const workspace = (store.state.workspaces || []).find(
		(w) => w.code === workspaceCode,
	);
	if (!workspace) return null;
	await deps.syncActiveLocalWorkspace(workspace.id);
	store.commit('updateUserWorkspaceSetting', { workspaceId: workspace.id });
	const [userToggles, workspaceToggles] = await Promise.all([
		deps.getUserFeatureToggles(),
		deps.getWorkspaceFeatureToggles(workspace.id),
	]);
	store.commit('featureToggles/setUserToggles', userToggles);
	store.commit('featureToggles/setWorkspaceToggles', workspaceToggles);
	return workspace;
};

export const bootstrapTaskWindow = async (
	store: BootstrapStore,
	deps: TaskWindowBootstrapDeps,
	workspaceCode: string,
	taskId: number,
): Promise<Workspace | null> => {
	if (!Number.isFinite(taskId) || taskId <= 0) return null;
	const workspace = await bootstrapWindowWorkspace(store, deps, workspaceCode);
	if (!workspace) return null;
	store.commit('setCurrentTaskIdForModal', taskId);
	return workspace;
};
