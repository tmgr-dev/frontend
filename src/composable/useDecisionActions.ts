import { onBeforeUnmount, watch } from 'vue';

interface ActionPayload {
	origin?: string;
	user_id?: number;
	task_id?: number;
	text?: string;
	workspace_id?: number;
	category_id?: number | null;
	status_id?: number | null;
	title?: string;
	description?: string;
}

export interface ActionIn {
	kind: string;
	payload: ActionPayload;
}

export interface PendingAction {
	run_id: string;
	card_id: string;
	action: ActionIn;
}

export type ClaimedAction = PendingAction;

export interface ActionResult {
	ok: boolean;
	url?: string;
	sha?: string;
	error?: string;
}

export interface DecisionActionDeps {
	invoke: (cmd: string, args?: Record<string, unknown>) => Promise<unknown>;
	postComment: (taskId: number, text: string) => Promise<unknown>;
	createTask: (fields: Record<string, unknown>) => Promise<{ id: number }>;
	taskWorkspaceId: (taskId: number) => Promise<number | null>;
	workspaceCode: (workspaceId: number) => string | null;
	webOrigin: string;
	origin: () => string | null;
	userId: () => number | null;
}

export const MISMATCH_ERROR =
	'This desktop is signed in to a different TMGR account or server';

const DEBOUNCE_MS = 500;
const INTERVAL_MS = 30 * 1000;

const apiOrigin = (baseURL: string | undefined): string | null => {
	try {
		return new URL(baseURL ?? '').origin;
	} catch {
		return null;
	}
};

const errorMessage = (error: unknown): string => {
	const e = error as {
		response?: { data?: { message?: string } };
		message?: string;
	};
	return e?.response?.data?.message ?? e?.message ?? String(error);
};

export const createDecisionActionRunner = (deps: DecisionActionDeps) => {
	const seen = new Set<string>();
	let inFlight: Promise<void> | null = null;
	let debounce: ReturnType<typeof setTimeout> | undefined;

	const taskUrl = (workspaceId: number | null, taskId: number) => {
		const code = workspaceId === null ? null : deps.workspaceCode(workspaceId);
		return code ? `${deps.webOrigin}/${code}/tasks/${taskId}` : undefined;
	};

	const signedInAs = (p: ActionPayload) => {
		const origin = deps.origin();
		const userId = deps.userId();
		return (
			origin !== null &&
			p.origin === origin &&
			userId !== null &&
			p.user_id === userId
		);
	};

	const execute = async (action: ActionIn): Promise<ActionResult> => {
		const p = action.payload;
		if (action.kind === 'comment') {
			const workspaceId = await deps.taskWorkspaceId(p.task_id as number);
			if (!signedInAs(p)) return { ok: false, error: MISMATCH_ERROR };
			await deps.postComment(p.task_id as number, p.text as string);
			const url = taskUrl(workspaceId, p.task_id as number);
			return url ? { ok: true, url } : { ok: true };
		}
		if (!signedInAs(p)) return { ok: false, error: MISMATCH_ERROR };
		const task = await deps.createTask({
			title: p.title,
			description: p.description,
			workspace_id: p.workspace_id,
			...(p.status_id ? { status_id: p.status_id } : {}),
			status: 'created',
			is_daily_routine: false,
			...(p.category_id ? { project_category_id: p.category_id } : {}),
		});
		const url = taskUrl(p.workspace_id ?? null, task.id);
		return url ? { ok: true, url } : { ok: true };
	};

	const finish = async (runId: string, result: ActionResult) => {
		try {
			await deps.invoke('decisions_finish_action', { runId, result });
		} catch {}
	};

	const handle = async (item: PendingAction) => {
		if (item.action.kind !== 'comment' && item.action.kind !== 'task') return;
		if (seen.has(item.run_id)) return;
		seen.add(item.run_id);
		let claimed: ClaimedAction;
		try {
			claimed = (await deps.invoke('decisions_claim_action', {
				runId: item.run_id,
			})) as ClaimedAction;
		} catch {
			return;
		}
		if (!signedInAs(claimed.action.payload)) {
			await finish(claimed.run_id, { ok: false, error: MISMATCH_ERROR });
			return;
		}
		try {
			await finish(claimed.run_id, await execute(claimed.action));
		} catch (error) {
			await finish(claimed.run_id, {
				ok: false,
				error: errorMessage(error),
			});
		}
	};

	const runOnce = async () => {
		let pending: PendingAction[];
		try {
			pending =
				((await deps.invoke('decisions_pending_actions')) as PendingAction[]) ??
				[];
		} catch {
			return;
		}
		for (const item of pending) await handle(item);
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

const defaultDeps = async (): Promise<DecisionActionDeps> => {
	const [
		{ invoke },
		{ createComment },
		{ createTask, getTask },
		{ WEB_ORIGIN },
		{ default: $axios },
		{ default: store },
	] = await Promise.all([
		import('@tauri-apps/api/core'),
		import('@/actions/tmgr/comments'),
		import('@/actions/tmgr/tasks'),
		import('@/utils/taskLinks'),
		import('@/plugins/axios'),
		import('@/store'),
	]);
	return {
		invoke,
		postComment: (taskId, text) => createComment(taskId, { message: text }),
		createTask: (fields) =>
			createTask(fields as unknown as Parameters<typeof createTask>[0]),
		taskWorkspaceId: async (taskId) =>
			(await getTask(taskId, false)).workspace_id ?? null,
		workspaceCode: (id) =>
			(store.state.workspacesById as Record<number, { code?: string }>)?.[id]
				?.code ?? null,
		webOrigin: WEB_ORIGIN,
		origin: () => apiOrigin($axios.defaults.baseURL),
		userId: () => store.state.user?.id ?? null,
	};
};

export const useDecisionActions = (isActive: () => boolean) => {
	let stopAll: (() => void) | null = null;
	let generation = 0;

	const start = async () => {
		const mine = ++generation;
		const runner = createDecisionActionRunner(await defaultDeps());
		const { listen } = await import('@tauri-apps/api/event');
		const unlisten = await listen('decisions://changed', runner.trigger);
		if (mine !== generation) {
			unlisten();
			return;
		}
		const interval = setInterval(() => void runner.run(), INTERVAL_MS);
		void runner.run();
		stopAll = () => {
			runner.stop();
			clearInterval(interval);
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
