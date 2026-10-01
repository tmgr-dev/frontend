import { isPageWindowLabel } from './pageWindow';

export interface TaskWindowTarget {
	taskId: number;
	workspaceCode: string;
	title: string | null;
}

interface TaskLike {
	id?: number | string | null;
	title?: string | null;
	workspace_id?: number | string | null;
}

interface WorkspaceLike {
	id?: number | string | null;
	code?: string | null;
}

export const isTaskWindowLabel = (label: string | null): boolean =>
	typeof label === 'string' && label.startsWith('task-');

export const isDetachedWindowLabel = (label: string | null): boolean =>
	isTaskWindowLabel(label) || isPageWindowLabel(label);

export const isInDetachedWindow = (): boolean =>
	isDetachedWindowLabel(
		(globalThis as any).__TAURI_INTERNALS__?.metadata?.currentWindow?.label ??
			null,
	);

export const isInSecondaryWindow = (): boolean =>
	isInDetachedWindow() ||
	(globalThis as any).__TAURI_INTERNALS__?.metadata?.currentWindow?.label ===
		'quick-add';

export const isInTaskWindow = (): boolean =>
	isTaskWindowLabel(
		(globalThis as any).__TAURI_INTERNALS__?.metadata?.currentWindow?.label ??
			null,
	);

export const taskWindowTarget = (
	task: TaskLike | null | undefined,
	workspaces: WorkspaceLike[] | null | undefined,
	currentWorkspace: WorkspaceLike | null | undefined,
): TaskWindowTarget | null => {
	const taskId = Number(task?.id);
	if (!task || !Number.isFinite(taskId) || taskId === 0) return null;
	const workspace =
		(workspaces ?? []).find(
			(w) => task.workspace_id != null && Number(w.id) === Number(task.workspace_id),
		) ?? currentWorkspace;
	if (!workspace?.code) return null;
	return {
		taskId,
		workspaceCode: workspace.code,
		title: typeof task.title === 'string' && task.title.trim() ? task.title.trim() : null,
	};
};

export const openTaskWindow = async (target: TaskWindowTarget): Promise<void> => {
	const { invoke } = await import('@tauri-apps/api/core');
	await invoke('open_task_window', { ...target });
};

export const focusTaskWindow = async (
	taskId: number,
	workspaceCode: string,
): Promise<boolean> => {
	const { invoke } = await import('@tauri-apps/api/core');
	return invoke<boolean>('focus_task_window', { taskId, workspaceCode });
};

export const setTaskWindowTitle = async (title: string): Promise<void> => {
	try {
		const { getCurrentWindow } = await import('@tauri-apps/api/window');
		await getCurrentWindow().setTitle(title.replace(/\s+/g, ' '));
	} catch {
		/* the native title is cosmetic */
	}
};
