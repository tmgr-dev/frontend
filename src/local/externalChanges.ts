import type { AgentWorkRun } from '@/actions/tmgr/agentWork';
import type { EventHandlers } from '@/types/dashboard';
import type { LocalDb } from './types';

export const EXTERNAL_CHANGE_EVENT = 'tmgr://local-db-changed';

const BULK_THRESHOLD = 20;
const RESYNC_INTERVAL_MS = 10_000;

interface ActiveWorkspace {
	id: number;
	code: string;
}

export interface ExternalChangeDeps {
	active: () => ActiveWorkspace | null;
	subscribeActive: (listener: () => void) => () => void;
	listen: (handler: (code: string) => void) => Promise<() => void>;
	getDb: (workspace: ActiveWorkspace) => Promise<LocalDb>;
	fetchTask: (workspaceId: number, taskId: number) => Promise<any | null>;
	fetchAgentRuns: (
		workspaceId: number,
		taskId: number,
	) => Promise<AgentWorkRun[]>;
	deliver: (
		workspaceId: number,
		call: (handlers: EventHandlers) => void,
	) => void;
	deliverToUser: (call: (handlers: EventHandlers) => void) => void;
	invalidate: (key: string | RegExp) => void;
	reloadActiveTasks: () => void;
}

interface Snapshot {
	code: string;
	tasks: Map<number, { stamp: string; deleted: boolean }>;
	comments: Map<number, { taskId: number; stamp: string }>;
	runs: Map<number, { taskId: number; stamp: string }>;
	fingerprints: Record<string, string>;
}

const FINGERPRINTS: Record<string, string> = {
	statuses:
		'SELECT COUNT(*) AS n, MAX(updated_at) AS u, MAX(id) AS m, SUM(id) AS s FROM statuses',
	categories:
		'SELECT COUNT(*) AS n, MAX(updated_at) AS u, MAX(id) AS m, COUNT(deleted_at) AS d FROM categories',
	reactions: 'SELECT COUNT(*) AS n, MAX(id) AS m FROM comment_reactions',
	relations: 'SELECT COUNT(*) AS n, MAX(id) AS m FROM task_relations',
};

const take = async (
	db: LocalDb,
	workspace: ActiveWorkspace,
): Promise<Snapshot> => {
	const tasks = new Map<number, { stamp: string; deleted: boolean }>();
	for (const row of await db.select<any>(
		'SELECT id, updated_at, deleted_at FROM tasks',
	))
		tasks.set(Number(row.id), {
			stamp: `${row.updated_at}|${row.deleted_at ?? ''}`,
			deleted: row.deleted_at != null,
		});
	const comments = new Map<number, { taskId: number; stamp: string }>();
	for (const row of await db.select<any>(
		'SELECT id, task_id, updated_at, deleted_at FROM comments',
	))
		comments.set(Number(row.id), {
			taskId: Number(row.task_id),
			stamp: `${row.updated_at}|${row.deleted_at ?? ''}`,
		});
	const runs = new Map<number, { taskId: number; stamp: string }>();
	for (const row of await db.select<any>(
		'SELECT id, task_id, version, updated_at FROM agent_work_runs',
	))
		runs.set(Number(row.id), {
			taskId: Number(row.task_id),
			stamp: `${row.version}|${row.updated_at}`,
		});
	const fingerprints: Record<string, string> = {};
	for (const [name, sql] of Object.entries(FINGERPRINTS))
		fingerprints[name] = JSON.stringify((await db.select<any>(sql))[0]);
	return { code: workspace.code, tasks, comments, runs, fingerprints };
};

/**
 * The tmgr CLI and other processes write to a local workspace's workspace.db behind this window's back.
 * The shell reports that (`tmgr://local-db-changed`, only for writes by another connection); this diffs the
 * database against an in-memory snapshot and hands the result to the same handlers the cloud realtime
 * events call. The snapshot is not updated by this window's own writes, so the first external event after
 * own edits can refetch those few tasks again (idempotent) and counts as a non-empty diff.
 */
export const installExternalChanges = async (
	deps: ExternalChangeDeps,
): Promise<() => void> => {
	let snapshot: Snapshot | null = null;
	let running = false;
	let rerun = false;
	let lastResync = 0;
	let resyncTimer: ReturnType<typeof setTimeout> | null = null;

	const resync = () => {
		const fire = () => {
			resyncTimer = null;
			lastResync = Date.now();
			const workspace = deps.active();
			if (!workspace) return;
			deps.deliver(workspace.id, (h) => h.onReconnect?.());
			deps.deliverToUser((h) => h.onReconnect?.());
		};
		if (resyncTimer) return;
		const wait = lastResync + RESYNC_INTERVAL_MS - Date.now();
		if (wait <= 0) fire();
		else resyncTimer = setTimeout(fire, wait);
	};

	const sameWorkspace = (workspace: ActiveWorkspace) =>
		deps.active()?.code === workspace.code;

	const diff = async (previous: Snapshot, current: Snapshot, id: number) => {
		const touchedTasks = new Set<number>();
		const gone: number[] = [];
		const created = new Set<number>();
		for (const [taskId, now] of current.tasks) {
			const before = previous.tasks.get(taskId);
			if (before?.stamp === now.stamp) continue;
			if (now.deleted) {
				if (!before?.deleted) gone.push(taskId);
			} else {
				touchedTasks.add(taskId);
				if (!before) created.add(taskId);
			}
		}
		for (const [taskId, before] of previous.tasks)
			if (!current.tasks.has(taskId) && !before.deleted) gone.push(taskId);

		const commentTasks = new Set<number>();
		for (const [commentId, now] of current.comments) {
			const before = previous.comments.get(commentId);
			if (before?.stamp !== now.stamp) commentTasks.add(now.taskId);
		}
		for (const [commentId, before] of previous.comments)
			if (!current.comments.has(commentId)) commentTasks.add(before.taskId);

		const changedRuns: { id: number; taskId: number }[] = [];
		for (const [runId, now] of current.runs)
			if (previous.runs.get(runId)?.stamp !== now.stamp)
				changedRuns.push({ id: runId, taskId: now.taskId });

		const changed = (name: string) =>
			previous.fingerprints[name] !== current.fingerprints[name];
		const statusesChanged = changed('statuses');
		const categoriesChanged = changed('categories');
		const reactionsChanged = changed('reactions');
		const relationsChanged = changed('relations');

		const refetch = new Set([...touchedTasks, ...commentTasks]);
		for (const taskId of gone) refetch.delete(taskId);
		const empty =
			!refetch.size &&
			!gone.length &&
			!changedRuns.length &&
			!statusesChanged &&
			!categoriesChanged &&
			!reactionsChanged &&
			!relationsChanged;
		if (empty) return;

		if (statusesChanged) {
			deps.invalidate('statuses');
			deps.invalidate(/^statuses-workspace-/);
			deps.invalidate('workspace-statuses');
		}
		if (categoriesChanged) {
			deps.invalidate('categories');
			deps.invalidate(/^categories-workspace-/);
		}
		if (reactionsChanged) deps.invalidate(/^comments-task-/);
		if (reactionsChanged || relationsChanged) deps.invalidate(/^task-\d+$/);
		for (const taskId of commentTasks) {
			deps.invalidate(`comments-task-${taskId}`);
			deps.deliver(id, (h) => h.onCommentUpdated?.({ task_id: taskId }));
		}

		if (refetch.size + gone.length > BULK_THRESHOLD) {
			deps.invalidate(/^task-\d+$/);
			deps.invalidate(/^tasks-status-/);
			deps.reloadActiveTasks();
		} else {
			for (const taskId of [...refetch, ...gone]) {
				deps.invalidate(`task-${taskId}`);
			}
			if (refetch.size || gone.length) deps.invalidate(/^tasks-status-/);
			for (const taskId of gone)
				deps.deliver(id, (h) => h.onTaskUpdated?.({ id: taskId }, 'deleted'));
			for (const taskId of refetch) {
				try {
					const task = await deps.fetchTask(id, taskId);
					if (task)
						deps.deliver(id, (h) =>
							h.onTaskUpdated?.(
								task,
								created.has(taskId) ? 'created' : 'updated',
							),
						);
				} catch (error) {
					console.error('[local-external] failed to refetch a task', error);
				}
			}
		}

		const runTasks = new Set(changedRuns.map((run) => run.taskId));
		for (const taskId of runTasks) {
			try {
				const wanted = new Set(
					changedRuns.filter((r) => r.taskId === taskId).map((r) => r.id),
				);
				for (const run of await deps.fetchAgentRuns(id, taskId))
					if (wanted.has(run.id))
						deps.deliver(id, (h) => h.onAgentWorkChanged?.(run));
			} catch (error) {
				console.error('[local-external] failed to fetch agent runs', error);
			}
		}
		resync();
	};

	const once = async () => {
		const workspace = deps.active();
		if (!workspace) {
			snapshot = null;
			return;
		}
		const current = await take(await deps.getDb(workspace), workspace);
		if (!sameWorkspace(workspace)) return;
		const previous = snapshot?.code === workspace.code ? snapshot : null;
		snapshot = current;
		if (previous) await diff(previous, current, workspace.id);
	};

	const request = async () => {
		if (running) {
			rerun = true;
			return;
		}
		running = true;
		try {
			do {
				rerun = false;
				try {
					await once();
				} catch (error) {
					console.error('[local-external] failed to read changes', error);
				}
			} while (rerun);
		} finally {
			running = false;
		}
	};

	const offActive = deps.subscribeActive(() => {
		snapshot = null;
		void request();
	});
	const offListen = await deps.listen((code) => {
		if (deps.active()?.code === code) void request();
	});
	await request();
	return () => {
		offActive();
		offListen();
		if (resyncTimer) clearTimeout(resyncTimer);
		resyncTimer = null;
	};
};
