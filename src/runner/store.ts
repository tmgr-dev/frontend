import { computed, ref } from 'vue';
import {
	isRunnerSupported,
	runnerEnabled,
	runnerSessions,
	runnerSetEnabled,
	runnerStatus,
	runnerUnwatch,
	runnerWatch,
	type RunnerSession,
	type RunnerStatus,
	type WatchEvent,
} from './api';
import { sanitizeState } from './states';

const enabled = ref(false);
const status = ref<RunnerStatus | null>(null);
const sessions = ref<RunnerSession[]>([]);

let watchId: number | null = null;
let generation = 0;
let attempt = 0;
let watching: Promise<void> | null = null;
let reconnectTimer: ReturnType<typeof setTimeout> | null = null;

const RECONNECT_MS = 3000;

const stopReconnect = (): void => {
	if (reconnectTimer !== null) clearTimeout(reconnectTimer);
	reconnectTimer = null;
};

export const taskKey = (workspaceId: number, taskId: number): string =>
	`${workspaceId}:${taskId}`;

const clean = (session: RunnerSession): RunnerSession => ({
	...session,
	state: sanitizeState(session.state),
});

const byTask = computed(() => {
	const map = new Map<string, RunnerSession[]>();
	for (const session of sessions.value) {
		if (!session.task) continue;
		const key = taskKey(session.task.workspaceId, session.task.taskId);
		map.set(key, [...(map.get(key) ?? []), session]);
	}
	for (const list of map.values())
		list.sort((a, b) => b.createdMs - a.createdMs);
	return map;
});

export const refreshRunnerStatus = async (): Promise<void> => {
	if (!enabled.value) return;
	try {
		status.value = await runnerStatus();
	} catch (error) {
		status.value = { state: 'absent', caps: [], message: String(error) };
	}
};

const scheduleReconnect = (): void => {
	stopReconnect();
	const mine = generation;
	reconnectTimer = setTimeout(async () => {
		reconnectTimer = null;
		if (mine !== generation || !enabled.value) return;
		try {
			const next = await runnerStatus();
			if (mine !== generation || !enabled.value) return;
			if (next.state === 'ready') {
				await beginWatch();
				return;
			}
		} catch {
			// the runner is still away
		}
		if (mine === generation && enabled.value) scheduleReconnect();
	}, RECONNECT_MS);
};

const onEvent = (token: number, event: WatchEvent): void => {
	if (token !== attempt) return;
	switch (event.type) {
		case 'state':
			sessions.value = sessions.value.map((session) =>
				session.id === event.session
					? { ...session, state: sanitizeState(event.state) }
					: session,
			);
			break;
		case 'sessions':
			sessions.value = event.sessions.map(clean);
			if (status.value?.state !== 'ready') void refreshRunnerStatus();
			break;
		case 'down':
			attempt++;
			watching = null;
			status.value = { state: 'absent', caps: [], message: event.message };
			sessions.value = [];
			if (watchId !== null) void runnerUnwatch(watchId).catch(() => {});
			watchId = null;
			scheduleReconnect();
			break;
	}
};

const beginWatch = async (): Promise<void> => {
	if (watchId !== null || watching) return watching ?? undefined;
	const token = ++attempt;
	const run = (async () => {
		const id = await runnerWatch((event) => onEvent(token, event));
		if (token !== attempt || !enabled.value) {
			await runnerUnwatch(id).catch(() => {});
			return;
		}
		watchId = id;
		await refreshRunnerStatus();
		try {
			sessions.value = (await runnerSessions()).map(clean);
		} catch {
			sessions.value = [];
		}
	})().finally(() => {
		if (watching === run) watching = null;
	});
	watching = run;
	return run;
};

const endWatch = async (): Promise<void> => {
	generation++;
	attempt++;
	watching = null;
	stopReconnect();
	const id = watchId;
	watchId = null;
	status.value = null;
	sessions.value = [];
	if (id !== null) await runnerUnwatch(id).catch(() => {});
};

export const startRunnerStore = async (): Promise<void> => {
	if (!isRunnerSupported()) return;
	try {
		enabled.value = await runnerEnabled();
	} catch {
		enabled.value = false;
	}
	if (enabled.value) await beginWatch();
};

export const setRunnerEnabled = async (value: boolean): Promise<void> => {
	await runnerSetEnabled(value);
	enabled.value = value;
	if (value) await beginWatch();
	else await endWatch();
};

export const resetRunnerStore = (): void => {
	generation++;
	attempt++;
	stopReconnect();
	watchId = null;
	watching = null;
	enabled.value = false;
	status.value = null;
	sessions.value = [];
};

export const useRunner = () => ({
	enabled,
	status,
	sessions,
	sessionForTask: (workspaceId: number, taskId: number) =>
		byTask.value.get(taskKey(workspaceId, taskId))?.[0] ?? null,
	sessionsForTask: (workspaceId: number, taskId: number) =>
		byTask.value.get(taskKey(workspaceId, taskId)) ?? [],
});
