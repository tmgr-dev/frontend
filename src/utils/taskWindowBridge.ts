export const TASK_WINDOW_NAVIGATE = 'task-window://navigate';
export const TASK_WINDOW_CREATE_TASK = 'task-window://create-task';

export interface CreateTaskRequest {
	statusId?: number | null;
	projectCategoryId?: number | null;
}

export const sendToMainWindow = async (
	channel: string,
	payload: Record<string, unknown>,
): Promise<void> => {
	const { emitTo } = await import('@tauri-apps/api/event');
	await emitTo('main', channel, payload);
};

const showMainWindow = async () => {
	const { getCurrentWindow } = await import('@tauri-apps/api/window');
	const win = getCurrentWindow();
	await win.show();
	await win.unminimize();
	await win.setFocus();
};

interface HostRouter {
	push: (path: string) => unknown;
}

interface HostStore {
	commit: (mutation: string, payload?: unknown) => void;
}

export const installTaskWindowHost = async (router: HostRouter, store: HostStore) => {
	const { listen } = await import('@tauri-apps/api/event');
	await listen<{ path?: string }>(TASK_WINDOW_NAVIGATE, ({ payload }) => {
		void showMainWindow().catch(() => {});
		if (typeof payload?.path === 'string') void router.push(payload.path);
	});
	await listen<CreateTaskRequest>(TASK_WINDOW_CREATE_TASK, ({ payload }) => {
		void showMainWindow().catch(() => {});
		store.commit('setShowCreatingTaskModal', payload?.statusId);
		store.commit('createTaskInProjectCategoryId', {
			projectCategoryId: payload?.projectCategoryId,
			statusId: payload?.statusId,
		});
	});
};

export const installTaskWindowNavigationGuard = (
	router: {
		beforeEach: (
			guard: (to: { name?: unknown; fullPath: string }) => boolean | void,
		) => () => void;
	},
	send: (path: string) => unknown = (path) => sendToMainWindow(TASK_WINDOW_NAVIGATE, { path }),
) =>
	router.beforeEach((to) => {
		if (to.name === 'TaskWindow') return;
		void send(to.fullPath);
		return false;
	});
