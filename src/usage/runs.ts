import $axios from '@/plugins/axios';
import type { RawRun } from './api';

export interface WorkspaceRef {
	id: number;
	code: string;
}

export interface TaskInfo {
	key: string;
	title: string | null;
	path: string;
	workspace: string;
}

export const stripKeyPrefix = (
	key: string,
	title: string | null,
): string | null => {
	if (!title) return title;
	const lower = title.toLowerCase();
	const k = key.toLowerCase();
	if (!lower.startsWith(k)) return title;
	const rest = title.slice(key.length);
	if (!/^(:|\s)/.test(rest)) return title;
	const clean = rest.replace(/^:?\s*/, '');
	return clean || title;
};

export interface FetchedRuns {
	runs: RawRun[];
	tasks: Record<string, TaskInfo>;
	failedWorkspaces: number;
	truncated: number;
}

const DAY_MS = 86_400_000;
const PAGE_LIMIT = 200;
const MAX_PAGES = 10;

export const taskRef = (workspace: string, taskId: number): string =>
	`${workspace}/${taskId}`;

const pageOf = (body: any): { items: any[]; cursor: string | null } => {
	const data = body?.data ?? body;
	const items = Array.isArray(data)
		? data
		: Array.isArray(data?.runs)
		? data.runs
		: Array.isArray(data?.data)
		? data.data
		: [];
	const cursor =
		data?.next_cursor ?? body?.next_cursor ?? body?.meta?.next_cursor ?? null;
	return {
		items,
		cursor: typeof cursor === 'string' && cursor ? cursor : null,
	};
};

export const toRawRun = (item: any, workspace: string): RawRun => ({
	id: Number(item.id),
	task_id: Number(item.task_id ?? item.task?.id),
	workspace,
	status: item.status ?? null,
	started_at: item.started_at,
	ended_at: item.ended_at ?? null,
	duration_seconds: item.duration_seconds ?? null,
	session_id: item.session_id ?? null,
	branch: item.branch ?? null,
});

const taskInfo = (item: any, ws: WorkspaceRef, taskId: number): TaskInfo => {
	const task = item.task;
	const key = task?.key ?? task?.code ?? `#${taskId}`;
	return {
		key,
		title: stripKeyPrefix(key, task?.title ?? null),
		path: `/${ws.code}/tasks/${taskId}`,
		workspace: ws.code,
	};
};

export async function fetchRuns(
	workspaces: WorkspaceRef[],
	now: Date = new Date(),
): Promise<FetchedRuns> {
	const since = new Date(now.getTime() - 33 * DAY_MS).toISOString();
	const until = now.toISOString();
	const result: FetchedRuns = {
		runs: [],
		tasks: {},
		failedWorkspaces: 0,
		truncated: 0,
	};
	for (const ws of workspaces) {
		if (!(ws.id >= 0)) continue;
		const runs: RawRun[] = [];
		const tasks: Record<string, TaskInfo> = {};
		let leftover = false;
		try {
			let cursor: string | null = null;
			for (let page = 0; page < MAX_PAGES; page++) {
				const params: Record<string, string | number> = {
					since,
					until,
					limit: PAGE_LIMIT,
				};
				if (cursor) params.cursor = cursor;
				const response: { data: any } = await $axios.get(
					`workspaces/${ws.id}/agent-work`,
					{ params },
				);
				const { items, cursor: next } = pageOf(response.data);
				for (const item of items) {
					const run = toRawRun(item, ws.code);
					if (
						item.id == null ||
						!Number.isFinite(run.id) ||
						!Number.isFinite(run.task_id) ||
						!run.started_at
					)
						continue;
					runs.push(run);
					const ref = taskRef(ws.code, run.task_id);
					if (!tasks[ref] || (item.task && !tasks[ref].title))
						tasks[ref] = taskInfo(item, ws, run.task_id);
				}
				if (!next || items.length === 0) break;
				cursor = next;
				if (page === MAX_PAGES - 1) leftover = true;
			}
		} catch {
			result.failedWorkspaces++;
			continue;
		}
		if (leftover) result.truncated++;
		result.runs.push(...runs);
		Object.assign(result.tasks, tasks);
	}
	return result;
}

export const taskInfoFor = (
	tasks: Record<string, TaskInfo>,
	ref: string,
): TaskInfo => {
	const known = tasks[ref];
	if (known) return known;
	const slash = ref.lastIndexOf('/');
	const workspace = ref.slice(0, slash);
	const id = ref.slice(slash + 1);
	return {
		key: `#${id}`,
		title: null,
		path: `/${workspace}/tasks/${id}`,
		workspace,
	};
};
