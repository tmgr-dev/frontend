import { isDesktopApp } from '@/utils/desktop';
import { listLocalWorkspaces, localContext } from '../runtime';
import type { LocalContext, LocalUser, LocalWorkspace } from '../types';
import { expandRange, materializeDueInstances, type RoutineEntryJson } from './service';

export const POLL_INTERVAL_MS = 60_000;
const STORAGE_KEY = 'local.routines.reminded';

export interface SchedulerStorage {
	getItem(key: string): string | null;
	setItem(key: string, value: string): void;
}

export interface SchedulerDeps {
	now: () => Date;
	notify: (title: string, body: string) => void;
	storage: SchedulerStorage;
	listWorkspaces: () => Promise<LocalWorkspace[]>;
	context: (workspace: LocalWorkspace) => Promise<LocalContext>;
}

const localDateKey = (d: Date): string => {
	const y = d.getFullYear();
	const m = String(d.getMonth() + 1).padStart(2, '0');
	const day = String(d.getDate()).padStart(2, '0');
	return `${y}-${m}-${day}`;
};

const withSeconds = (time: string): string =>
	time.split(':').length === 3 ? time : `${time}:00`;

const entryScheduledAt = (entry: RoutineEntryJson): Date | null => {
	if (!entry.time) return null;
	const date = new Date(`${entry.date}T${withSeconds(entry.time)}`);
	return Number.isNaN(date.getTime()) ? null : date;
};

export const reminderKey = (workspaceCode: string, entry: RoutineEntryJson): string =>
	`${workspaceCode}:${entry.task_id}:${entry.date}:${entry.time}`;

/** Mirrors TaskScheduleProcessor.maybeRemind: fire once the reminder window opens, never after the routine's own time. */
export const dueReminders = (
	entries: RoutineEntryJson[],
	now: Date,
): RoutineEntryJson[] =>
	entries.filter((entry) => {
		if (String(entry.status).toUpperCase() !== 'PENDING') return false;
		if (entry.reminder_min == null) return false;
		const scheduled = entryScheduledAt(entry);
		if (!scheduled) return false;
		const fireAt = scheduled.getTime() - entry.reminder_min * 60_000;
		const nowMs = now.getTime();
		return nowMs >= fireAt && nowMs < scheduled.getTime();
	});

type RemindedStore = Record<string, string>;

const loadReminded = (storage: SchedulerStorage): RemindedStore => {
	try {
		const raw = storage.getItem(STORAGE_KEY);
		return raw ? JSON.parse(raw) : {};
	} catch {
		return {};
	}
};

const saveReminded = (storage: SchedulerStorage, reminded: RemindedStore): void => {
	try {
		storage.setItem(STORAGE_KEY, JSON.stringify(reminded));
	} catch {
		/* storage full or unavailable: a reminder may repeat until it recovers */
	}
};

/** Keeps only keys for today or later, so the store never grows across days. */
export const pruneReminded = (reminded: RemindedStore, todayIso: string): RemindedStore => {
	const kept: RemindedStore = {};
	for (const [key, date] of Object.entries(reminded)) {
		if (date >= todayIso) kept[key] = date;
	}
	return kept;
};

export const runSchedulerTick = async (deps: SchedulerDeps): Promise<void> => {
	const workspaces = await deps.listWorkspaces();
	const now = deps.now();
	const todayIso = localDateKey(now);
	const tomorrow = new Date(now.getFullYear(), now.getMonth(), now.getDate() + 1);
	const tomorrowIso = localDateKey(tomorrow);
	let reminded = pruneReminded(loadReminded(deps.storage), todayIso);

	for (const workspace of workspaces) {
		try {
			const ctx = await deps.context(workspace);
			await materializeDueInstances(ctx);
			const entries = await expandRange(ctx, todayIso, tomorrowIso);
			for (const entry of dueReminders(entries, now)) {
				const key = reminderKey(workspace.code, entry);
				if (reminded[key]) continue;
				reminded[key] = entry.date;
				deps.notify(`Routine: ${entry.title}`, `at ${entry.time} · ${workspace.name} (local)`);
			}
		} catch (error) {
			console.error('routine scheduler failed for local workspace', workspace.code, error);
		}
	}

	saveReminded(deps.storage, reminded);
};

const defaultNotify = (title: string, body: string): void => {
	if (typeof Notification === 'undefined' || Notification.permission !== 'granted') return;
	new Notification(title, { body });
};

const requestNotificationPermissionOnce = (): void => {
	if (typeof Notification === 'undefined') return;
	if (Notification.permission === 'default') Notification.requestPermission();
};

const defaultStorage: SchedulerStorage = {
	getItem: (key) => localStorage.getItem(key),
	setItem: (key, value) => localStorage.setItem(key, value),
};

const userFromStore = async (): Promise<LocalUser> => {
	const { default: store } = await import('@/store');
	const user: any = store.state.user ?? {};
	return { id: Number(user.id ?? 0), name: user.name ?? '', email: user.email ?? '' };
};

let timer: ReturnType<typeof setInterval> | null = null;

/** Desktop only. Materializes due routine instances and fires local reminders, at startup and every 60s. */
export const startRoutineScheduler = (deps?: Partial<SchedulerDeps>): void => {
	if (!isDesktopApp() || timer) return;
	const resolved: SchedulerDeps = {
		now: deps?.now ?? (() => new Date()),
		notify: deps?.notify ?? defaultNotify,
		storage: deps?.storage ?? defaultStorage,
		listWorkspaces: deps?.listWorkspaces ?? (() => listLocalWorkspaces()),
		context:
			deps?.context ?? (async (workspace) => localContext(workspace, await userFromStore())),
	};
	requestNotificationPermissionOnce();
	void runSchedulerTick(resolved);
	timer = setInterval(() => void runSchedulerTick(resolved), POLL_INTERVAL_MS);
};

export const stopRoutineScheduler = (): void => {
	if (timer) clearInterval(timer);
	timer = null;
};
