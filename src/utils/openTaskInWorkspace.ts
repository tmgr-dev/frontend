interface WorkspaceRef {
	id: number | string;
	code: string;
}

interface WorkspaceStore {
	state: { workspaces?: WorkspaceRef[] };
	getters: { currentWorkspaceId: number | string | null | undefined };
	commit: (type: string, payload: unknown) => void;
}

interface Navigator {
	push: (location: string) => unknown;
}

export const openTaskInWorkspace = async (
	{
		taskId,
		workspaceId,
	}: { taskId: number | string; workspaceId?: number | string | null },
	store: WorkspaceStore,
	router: Navigator,
): Promise<void> => {
	if (
		workspaceId == null ||
		Number(workspaceId) === Number(store.getters.currentWorkspaceId)
	) {
		store.commit('setCurrentTaskIdForModal', taskId);
		return;
	}

	const workspace = store.state.workspaces?.find(
		(w) => Number(w.id) === Number(workspaceId),
	);
	if (!workspace) return;

	await router.push(`/${workspace.code}/list`);
	store.commit('setCurrentTaskIdForModal', taskId);
};
