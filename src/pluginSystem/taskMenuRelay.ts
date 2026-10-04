import { computed, ref, shallowRef } from 'vue';
import type { TaskMenuItem } from './taskMenu';

export const TASK_MENU_ITEMS = 'plugin-task-menu://items';
export const TASK_MENU_REQUEST = 'plugin-task-menu://request';
export const TASK_MENU_RUN = 'plugin-task-menu://run';
export const TASK_MENU_RESULT = 'plugin-task-menu://result';

export const TASK_MENU_RUN_TIMEOUT_MS = 6 * 60_000;

export interface TaskMenuRunRequest {
	requestId: string;
	label: string;
	pluginId: string;
	command: string;
	taskId: number;
	workspaceId: number;
}

export interface TaskMenuRunResult {
	requestId: string;
	error: string | null;
}

export type RelaySend = (
	target: string | null,
	channel: string,
	payload: Record<string, unknown>,
) => unknown;

const isRecord = (value: unknown): value is Record<string, unknown> =>
	typeof value === 'object' && value !== null && !Array.isArray(value);

const isPositiveInteger = (value: unknown): value is number =>
	typeof value === 'number' && Number.isSafeInteger(value) && value > 0;

const isText = (value: unknown): value is string =>
	typeof value === 'string' && value.length > 0;

export const parseTaskMenuItems = (payload: unknown): TaskMenuItem[] | null => {
	if (!isRecord(payload) || !Array.isArray(payload.items)) return null;
	const items: TaskMenuItem[] = [];
	for (const raw of payload.items) {
		if (
			!isRecord(raw) ||
			!isText(raw.pluginId) ||
			!isText(raw.pluginName) ||
			!isText(raw.command) ||
			!isText(raw.title)
		)
			return null;
		items.push({
			pluginId: raw.pluginId,
			pluginName: raw.pluginName,
			command: raw.command,
			title: raw.title,
		});
	}
	return items;
};

export const parseTaskMenuRequest = (
	payload: unknown,
): { label: string } | null =>
	isRecord(payload) && isText(payload.label) ? { label: payload.label } : null;

export const parseTaskMenuRun = (
	payload: unknown,
): TaskMenuRunRequest | null => {
	if (
		!isRecord(payload) ||
		!isPositiveInteger(payload.taskId) ||
		!isPositiveInteger(payload.workspaceId)
	)
		return null;
	if (
		!isRecord(payload) ||
		!isText(payload.requestId) ||
		!isText(payload.label) ||
		!isText(payload.pluginId) ||
		!isText(payload.command)
	)
		return null;
	return {
		requestId: payload.requestId,
		label: payload.label,
		pluginId: payload.pluginId,
		command: payload.command,
		taskId: payload.taskId as number,
		workspaceId: payload.workspaceId as number,
	};
};

export const parseTaskMenuResult = (
	payload: unknown,
): TaskMenuRunResult | null => {
	if (!isRecord(payload)) return null;
	const { requestId, error } = payload;
	if (!isText(requestId)) return null;
	if (error === null || typeof error === 'string')
		return { requestId: requestId as string, error: error as string | null };
	return null;
};

const errorMessage = (error: unknown) =>
	error instanceof Error ? error.message : String(error);

export interface TaskMenuRelayHostDeps {
	send: RelaySend;
	getItems: () => TaskMenuItem[];
	getWorkspaceId: () => number | null;
	run: (
		pluginId: string,
		command: string,
		taskId: number,
		workspaceId: number,
	) => Promise<unknown>;
}

/** Main-window side: answers snapshot requests and run requests from detached windows. */
export const createTaskMenuRelayHost = ({
	send,
	getItems,
	getWorkspaceId,
	run,
}: TaskMenuRelayHostDeps) => {
	const snapshot = () => ({ items: getItems(), workspaceId: getWorkspaceId() });
	return {
		broadcast: () => send(null, TASK_MENU_ITEMS, snapshot()),
		onRequest: (payload: unknown) => {
			const request = parseTaskMenuRequest(payload);
			if (request) send(request.label, TASK_MENU_ITEMS, snapshot());
		},
		onRun: async (payload: unknown) => {
			const request = parseTaskMenuRun(payload);
			if (!request) return;
			let error: string | null = null;
			try {
				await run(
					request.pluginId,
					request.command,
					request.taskId,
					request.workspaceId,
				);
			} catch (cause) {
				error = errorMessage(cause);
			}
			await send(request.label, TASK_MENU_RESULT, {
				requestId: request.requestId,
				error,
			});
		},
	};
};

export interface TaskMenuRelayClientDeps {
	label: string;
	send: RelaySend;
	workspaceId: () => number | null;
	timeoutMs?: number;
	nextId?: () => string;
}

/** Detached-window side: holds the last snapshot from the main window and correlates run results. */
export const createTaskMenuRelayClient = ({
	label,
	send,
	workspaceId,
	timeoutMs = TASK_MENU_RUN_TIMEOUT_MS,
	nextId = () =>
		`${Date.now().toString(36)}-${Math.random().toString(36).slice(2)}`,
}: TaskMenuRelayClientDeps) => {
	const snapshot = ref<{ items: TaskMenuItem[]; workspaceId: number | null }>({
		items: [],
		workspaceId: null,
	});
	const items = computed(() =>
		snapshot.value.workspaceId !== null &&
		snapshot.value.workspaceId === workspaceId()
			? snapshot.value.items
			: [],
	);
	const pending = new Map<string, (result: TaskMenuRunResult) => void>();

	return {
		items,
		requestSnapshot: () => send('main', TASK_MENU_REQUEST, { label }),
		onItems: (payload: unknown) => {
			const parsed = parseTaskMenuItems(payload);
			if (!parsed) return;
			const owner = (payload as Record<string, unknown>).workspaceId;
			snapshot.value = {
				items: parsed,
				workspaceId: isPositiveInteger(owner) ? owner : null,
			};
		},
		onResult: (payload: unknown) => {
			const result = parseTaskMenuResult(payload);
			if (!result) return;
			pending.get(result.requestId)?.(result);
		},
		run: (item: TaskMenuItem, taskId: number) =>
			new Promise<void>((resolve, reject) => {
				const currentWorkspaceId = workspaceId();
				if (currentWorkspaceId === null) {
					reject(new Error('The plugin menu is not available'));
					return;
				}
				const requestId = nextId();
				const timer = setTimeout(() => {
					pending.delete(requestId);
					reject(new Error('The main window did not answer'));
				}, timeoutMs);
				pending.set(requestId, (result) => {
					clearTimeout(timer);
					pending.delete(requestId);
					if (result.error === null) resolve();
					else reject(new Error(result.error));
				});
				Promise.resolve(
					send('main', TASK_MENU_RUN, {
						requestId,
						label,
						pluginId: item.pluginId,
						command: item.command,
						taskId,
						workspaceId: currentWorkspaceId,
					}),
				).catch((cause) => {
					clearTimeout(timer);
					pending.delete(requestId);
					reject(cause instanceof Error ? cause : new Error(String(cause)));
				});
			}),
	};
};

export type TaskMenuRelayClient = ReturnType<typeof createTaskMenuRelayClient>;

const client = shallowRef<TaskMenuRelayClient | null>(null);
export const detachedTaskMenuClient = () => client.value;

const tauriSend: RelaySend = async (target, channel, payload) => {
	const { emit, emitTo } = await import('@tauri-apps/api/event');
	if (target === null) await emit(channel, payload);
	else await emitTo(target, channel, payload);
};

export const installTaskMenuRelayHost = async (
	getItems: () => TaskMenuItem[],
	getWorkspaceId: () => number | null,
	run: TaskMenuRelayHostDeps['run'],
	watchItems: (onChange: () => void) => void,
) => {
	const { listen } = await import('@tauri-apps/api/event');
	const host = createTaskMenuRelayHost({
		send: tauriSend,
		getItems,
		getWorkspaceId,
		run,
	});
	await listen(
		TASK_MENU_REQUEST,
		({ payload }) =>
			void Promise.resolve(host.onRequest(payload)).catch(() => {}),
	);
	await listen(TASK_MENU_RUN, ({ payload }) => void host.onRun(payload));
	watchItems(() => void Promise.resolve(host.broadcast()).catch(() => {}));
};

export const installTaskMenuRelayClient = async (
	label: string,
	workspaceId: () => number | null,
) => {
	const { listen } = await import('@tauri-apps/api/event');
	const created = createTaskMenuRelayClient({
		label,
		send: tauriSend,
		workspaceId,
	});
	client.value = created;
	await listen(TASK_MENU_ITEMS, ({ payload }) => created.onItems(payload));
	await listen(TASK_MENU_RESULT, ({ payload }) => created.onResult(payload));
	await created.requestSnapshot();
};
