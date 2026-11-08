import { onBeforeUnmount, watch } from 'vue';

export interface PendingTaskComment {
	card_id: string;
	seq: number;
	task_id: number;
	origin: string;
	workspace: string;
	body: string;
}

export interface DecisionTaskCommentDeps {
	invoke: (cmd: string, args?: Record<string, unknown>) => Promise<unknown>;
	fetchComments: (taskId: number) => Promise<{ message: string }[]>;
	postComment: (taskId: number, message: string) => Promise<unknown>;
	skip: (cardId: string, seq: number, reason: string) => Promise<unknown>;
	taskWorkspaceCode: (taskId: number) => Promise<string | null>;
	now: () => number;
	origin: string | null;
}

export const BACKOFF_MS = 15 * 60 * 1000;
const DEBOUNCE_MS = 2000;
const INTERVAL_MS = 60 * 1000;

export const PROD_API_ORIGIN = 'https://api.tmgr.dev';

const apiOrigin = (baseURL: string | undefined): string | null => {
	try {
		return new URL(baseURL ?? '').origin;
	} catch {
		return null;
	}
};

const httpStatus = (error: unknown): number | undefined =>
	(error as { response?: { status?: number } })?.response?.status;

const marker = (item: PendingTaskComment) =>
	`[decision ${item.card_id}#${item.seq}]`;

export const createDecisionTaskCommentSync = (
	deps: DecisionTaskCommentDeps,
) => {
	const backoffUntil = new Map<string, number>();
	let inFlight: Promise<void> | null = null;
	let debounce: ReturnType<typeof setTimeout> | undefined;

	const deliver = async (item: PendingTaskComment) => {
		const key = `${item.card_id}#${item.seq}`;
		if ((backoffUntil.get(key) ?? 0) > deps.now()) return;
		if (deps.origin === null || item.origin !== deps.origin) return;
		try {
			const code = await deps.taskWorkspaceCode(item.task_id);
			if (code === null) {
				console.warn(
					`[decisions] answer ${key} not posted: workspace of task ${item.task_id} not determined`,
				);
				backoffUntil.set(key, deps.now() + BACKOFF_MS);
				return;
			}
			if (code !== item.workspace) {
				console.warn(
					`[decisions] answer ${key} not posted: task ${item.task_id} is in workspace ${code}, the card link says ${item.workspace}`,
				);
				await deps.skip(item.card_id, item.seq, 'workspace mismatch');
				return;
			}
			const comments = await deps.fetchComments(item.task_id);
			const present = comments.some((c) =>
				String(c.message ?? '').includes(marker(item)),
			);
			if (!present) await deps.postComment(item.task_id, item.body);
			await deps.invoke('decisions_mark_task_comment', {
				cardId: item.card_id,
				seq: item.seq,
			});
			backoffUntil.delete(key);
		} catch (error) {
			const status = httpStatus(error);
			if (
				status !== undefined &&
				status >= 400 &&
				status < 500 &&
				status !== 408 &&
				status !== 429
			) {
				backoffUntil.set(key, deps.now() + BACKOFF_MS);
			}
		}
	};

	const runOnce = async () => {
		let pending: PendingTaskComment[];
		try {
			pending =
				((await deps.invoke(
					'decisions_pending_task_comments',
				)) as PendingTaskComment[]) ?? [];
		} catch {
			return;
		}
		for (const item of pending) await deliver(item);
	};

	const run = (): Promise<void> => {
		if (!inFlight) {
			inFlight = runOnce().finally(() => {
				inFlight = null;
			});
		}
		return inFlight;
	};

	const trigger = () => {
		clearTimeout(debounce);
		debounce = setTimeout(() => void run(), DEBOUNCE_MS);
	};

	const stop = () => clearTimeout(debounce);

	return { run, trigger, stop };
};

const defaultDeps = async (): Promise<DecisionTaskCommentDeps> => {
	const [
		{ invoke },
		{ getComments, createComment },
		{ getTask },
		{ getWorkspaces },
		{ WEB_ORIGIN },
		{ default: $axios },
	] = await Promise.all([
		import('@tauri-apps/api/core'),
		import('@/actions/tmgr/comments'),
		import('@/actions/tmgr/tasks'),
		import('@/actions/tmgr/workspaces'),
		import('@/utils/taskLinks'),
		import('@/plugins/axios'),
	]);
	return {
		invoke,
		fetchComments: (taskId) => getComments(taskId, false),
		postComment: (taskId, message) => createComment(taskId, { message }),
		skip: (cardId, seq, reason) =>
			invoke('decisions_skip_task_comment', { cardId, seq, reason }),
		taskWorkspaceCode: async (taskId) => {
			const task = await getTask(taskId, false);
			if (task.workspace_id === undefined) return null;
			const workspaces = await getWorkspaces(false);
			return workspaces.find((w) => w.id === task.workspace_id)?.code ?? null;
		},
		now: Date.now,
		origin:
			apiOrigin($axios.defaults.baseURL) === PROD_API_ORIGIN
				? WEB_ORIGIN
				: null,
	};
};

export const useDecisionTaskComments = (isActive: () => boolean) => {
	let stopAll: (() => void) | null = null;
	let generation = 0;

	const start = async () => {
		const mine = ++generation;
		const sync = createDecisionTaskCommentSync(await defaultDeps());
		const { listen } = await import('@tauri-apps/api/event');
		const unlisten = await listen('decisions://changed', sync.trigger);
		if (mine !== generation) {
			unlisten();
			return;
		}
		const interval = setInterval(() => void sync.run(), INTERVAL_MS);
		const onOnline = () => void sync.run();
		window.addEventListener('online', onOnline);
		void sync.run();
		stopAll = () => {
			sync.stop();
			clearInterval(interval);
			window.removeEventListener('online', onOnline);
			unlisten();
			stopAll = null;
		};
	};

	const teardown = () => {
		generation++;
		stopAll?.();
	};

	watch(
		isActive,
		(active) => {
			teardown();
			if (active) void start();
		},
		{ immediate: true },
	);
	onBeforeUnmount(teardown);
};
