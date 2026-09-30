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

export const bootstrapTaskWindow = async (
	store: BootstrapStore,
	deps: TaskWindowBootstrapDeps,
	workspaceCode: string,
	taskId: number,
): Promise<Workspace | null> => {
	if (!store.state.workspaces?.length) {
		store.commit('setWorkspaces', await deps.getWorkspaces());
	}
	const workspace = (store.state.workspaces || []).find(
		(w) => w.code === workspaceCode,
	);
	if (!workspace || !Number.isFinite(taskId) || taskId <= 0) return null;
	await deps.syncActiveLocalWorkspace(workspace.id);
	store.commit('updateUserWorkspaceSetting', { workspaceId: workspace.id });
	const [userToggles, workspaceToggles] = await Promise.all([
		deps.getUserFeatureToggles(),
		deps.getWorkspaceFeatureToggles(workspace.id),
	]);
	store.commit('featureToggles/setUserToggles', userToggles);
	store.commit('featureToggles/setWorkspaceToggles', workspaceToggles);
	store.commit('setCurrentTaskIdForModal', taskId);
	return workspace;
};
