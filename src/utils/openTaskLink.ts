import type { TaskLinkRef } from './taskLinks';

interface WorkspaceRef {
	id: number | string;
	code: string;
}

export interface OpenTaskLinkDeps {
	loadWorkspaces: () => Promise<WorkspaceRef[]>;
	currentWorkspaceId: () => number | string | null | undefined;
	fetchTask: (taskId: number, workspaceId: number) => Promise<unknown>;
	focusTaskWindow: (taskId: number, workspaceCode: string) => Promise<boolean>;
	showMain: () => Promise<unknown>;
	navigate: (path: string) => Promise<unknown>;
	openModal: (taskId: number) => void;
}

export type TaskLinkFailure = 'workspace' | 'not_found' | 'forbidden' | 'error';

export type OpenTaskLinkResult =
	| { ok: true }
	| { ok: false; reason: TaskLinkFailure };

const MESSAGES: Record<TaskLinkFailure, string> = {
	workspace: "This link points to a workspace you don't have access to.",
	not_found: 'This task no longer exists.',
	forbidden: "You don't have access to this task.",
	error: "Couldn't load this task. Check your connection and try again.",
};

export const taskLinkFailureMessage = (reason: TaskLinkFailure): string =>
	MESSAGES[reason];

const failureOf = (error: unknown): TaskLinkFailure => {
	const status = (error as { response?: { status?: number } })?.response?.status;
	if (status === 404) return 'not_found';
	if (status === 403) return 'forbidden';
	return 'error';
};

export const openTaskLink = async (
	link: TaskLinkRef,
	deps: OpenTaskLinkDeps,
): Promise<OpenTaskLinkResult> => {
	const workspaces = await deps.loadWorkspaces().catch(() => []);
	const currentId = deps.currentWorkspaceId();
	const workspace = link.workspaceCode
		? workspaces.find((w) => w.code === link.workspaceCode)
		: workspaces.find((w) => Number(w.id) === Number(currentId));
	if (!workspace) {
		return { ok: false, reason: link.workspaceCode ? 'workspace' : 'error' };
	}

	const workspaceId = Number(workspace.id);
	if (workspaceId > 0) {
		try {
			await deps.fetchTask(link.taskId, workspaceId);
		} catch (error) {
			return { ok: false, reason: failureOf(error) };
		}
	}

	if (await deps.focusTaskWindow(link.taskId, workspace.code).catch(() => false)) {
		return { ok: true };
	}
	await deps.showMain();
	if (workspaceId !== Number(currentId)) {
		await deps.navigate(`/${workspace.code}/list`);
	}
	deps.openModal(link.taskId);
	return { ok: true };
};
