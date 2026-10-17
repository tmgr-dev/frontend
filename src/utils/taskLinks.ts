export const WEB_ORIGIN = 'https://tmgr.dev';

const WORKSPACE_CODE = '[A-Za-z0-9_-]{1,64}';
const APP_LINK = new RegExp(
	`^tmgr://task/(?:(${WORKSPACE_CODE})/)?(\\d{1,15})/?$`,
);
const WEB_LINK = new RegExp(
	`^https://tmgr\\.dev/(${WORKSPACE_CODE})/tasks/(\\d{1,15})/?(?:[?#].*)?$`,
);

export interface TaskLinkRef {
	workspaceCode: string | null;
	taskId: number;
}

export const buildWebTaskUrl = (
	workspaceCode: string,
	taskId: number,
): string => `${WEB_ORIGIN}/${workspaceCode}/tasks/${taskId}`;

export const buildAppTaskUrl = (
	workspaceCode: string,
	taskId: number,
): string => `tmgr://task/${workspaceCode}/${taskId}`;

export const buildMarkdownLink = (
	title: string | null | undefined,
	url: string,
): string => {
	const flat = (title || '').replace(/\s+/g, ' ').trim();
	const label = (flat || 'Task').replace(/([\\[\]])/g, '\\$1');
	return `[${label}](${url})`;
};

export const parseTaskLink = (url: string): TaskLinkRef | null => {
	const trimmed = (url || '').trim();
	const match = APP_LINK.exec(trimmed) ?? WEB_LINK.exec(trimmed);
	if (!match) return null;
	const taskId = Number(match[2]);
	if (!Number.isSafeInteger(taskId) || taskId <= 0) return null;
	return { workspaceCode: match[1] ?? null, taskId };
};

export interface TaskShareLinks {
	web: string | null;
	app: string;
	markdown: string;
}

export const taskShareLinks = (
	task: { id: number; title?: string | null },
	workspace: { id: number | string; code: string },
): TaskShareLinks => {
	const app = buildAppTaskUrl(workspace.code, task.id);
	const web =
		Number(workspace.id) < 0 ? null : buildWebTaskUrl(workspace.code, task.id);
	return { web, app, markdown: buildMarkdownLink(task.title, web ?? app) };
};
