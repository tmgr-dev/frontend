import { focusTaskWindow } from './taskWindow';

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

interface TaskRef {
	taskId: number | string;
	workspaceId?: number | string | null;
}

export const openTaskInWorkspace = async (
	{ taskId, workspaceId }: TaskRef,
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

export const openTaskPreferringWindow = async (
	ref: TaskRef,
	store: WorkspaceStore,
	router: Navigator,
	showMain?: () => unknown,
	focusWindow: (taskId: number, workspaceCode: string) => Promise<boolean> = focusTaskWindow,
): Promise<void> => {
	const workspaceId = ref.workspaceId ?? store.getters.currentWorkspaceId;
	const code = store.state.workspaces?.find(
		(w) => Number(w.id) === Number(workspaceId),
	)?.code;
	if (code && (await focusWindow(Number(ref.taskId), code).catch(() => false))) {
		return;
	}
	await showMain?.();
	await openTaskInWorkspace(ref, store, router);
};
