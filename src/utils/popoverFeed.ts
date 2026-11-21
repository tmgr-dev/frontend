import { fetchMachines } from '@/actions/tmgr/aiUsageMachines';
import {
	completeRoutineOn,
	expandRoutineRange,
} from '@/actions/tmgr/daily-tasks';
import { getDashboardStatistics } from '@/actions/tmgr/dashboard';
import { getStatusesOfWorkspace } from '@/actions/tmgr/statuses';
import type { Task } from '@/actions/tmgr/tasks';
import { createTask } from '@/actions/tmgr/tasks';
import { getWorkspaceTimeDaily } from '@/actions/tmgr/timeDaily';
import { t } from '@/i18n';
import { followActiveLocalWorkspace } from '@/local/runtime';
import type { PopoverExtras } from '@/popover/types';
import { isRunnerSupported, runnerSessions } from '@/runner/api';
import { usageOverview } from '@/usage/api';
import { pickDefaultStatusId } from '@/utils/defaultStatus';
import { isDesktopApp, supportsToastWindow } from '@/utils/desktop';
import { limitToasts, pushToast } from '@/utils/toastTriggers';
import { format } from 'date-fns';
import {
	buildMachines,
	buildTodayExtras,
	buildTodaySecondsByTask,
	buildUsageExtras,
	buildWorkspace,
} from './popoverFeedModel';

export {
	buildMachines,
	buildTodayExtras,
	buildTodaySecondsByTask,
	buildUsageExtras,
	buildWorkspace,
} from './popoverFeedModel';

export interface PopoverFeedOptions {
	store: { getters: Record<string, any> };
	recent: () => { id: number; workspaceId: number | null }[];
}

export interface PopoverFeed {
	refresh: () => void;
	stop: () => void;
}

const REFRESH_MS = 60_000;
const DEBOUNCE_MS = 300;

const safe = async <T>(load: () => Promise<T>): Promise<T | undefined> => {
	try {
		return await load();
	} catch (error) {
		console.error('popover feed source failed', error);
		return undefined;
	}
};

const trackedTodaySeconds = async (
	workspaceId: number,
): Promise<number | null> => {
	const result = await getDashboardStatistics(workspaceId);
	const seconds = result.data?.time_today;
	return typeof seconds === 'number' && Number.isFinite(seconds)
		? seconds
		: null;
};

const gatherExtras = async (
	store: PopoverFeedOptions['store'],
	recent: PopoverFeedOptions['recent'],
): Promise<PopoverExtras> => {
	const now = Date.now();
	const today = format(now, 'yyyy-MM-dd');
	const utcDay = new Date(now).toISOString().slice(0, 10);
	const workspaceId: number | null = store.getters.currentWorkspaceId ?? null;
	const recentIds =
		workspaceId === null
			? []
			: recent()
					.filter((task) => task.workspaceId === workspaceId)
					.map((task) => task.id);

	const [usage, trackedSeconds, routines, days, runner, cloud] =
		await Promise.all([
			safe(async () => buildUsageExtras(await usageOverview(), undefined, now)),
			workspaceId === null
				? undefined
				: safe(() => trackedTodaySeconds(workspaceId)),
			workspaceId === null
				? undefined
				: safe(() => expandRoutineRange(today, today)),
			workspaceId === null || recentIds.length === 0
				? undefined
				: safe(() => getWorkspaceTimeDaily(workspaceId, utcDay, utcDay)),
			isRunnerSupported() ? safe(() => runnerSessions()) : undefined,
			safe(() => fetchMachines()),
		]);

	return {
		workspace: buildWorkspace(store.getters.currentWorkspace),
		today: buildTodayExtras({ trackedSeconds, routines, today }),
		todaySecondsByTask:
			workspaceId === null || !days
				? undefined
				: buildTodaySecondsByTask(workspaceId, recentIds, days, utcDay),
		usage,
		machines: buildMachines(
			runner ?? null,
			cloud ?? null,
			now,
			t('desktop.popover.localMachine'),
		),
	};
};

export const createQuickTask = async (
	store: PopoverFeedOptions['store'],
	title: string,
): Promise<void> => {
	const workspaceId: number = store.getters.currentWorkspaceId;
	followActiveLocalWorkspace(Boolean(store.getters.currentWorkspace?.is_local));
	const statuses = await getStatusesOfWorkspace(workspaceId);
	await createTask({
		title: title.trim(),
		workspace_id: workspaceId,
		status: 'created',
		status_id: pickDefaultStatusId(statuses),
		is_daily_routine: false,
	} as unknown as Task);
};

export const completeTodayRoutine = async (
	taskId: number,
): Promise<boolean> => {
	const today = format(new Date(), 'yyyy-MM-dd');
	const entries = await expandRoutineRange(today, today);
	if (!entries.some((entry) => entry.task_id === taskId && !entry.completed)) {
		return false;
	}
	await completeRoutineOn(taskId, today);
	return true;
};

export const startPopoverFeed = ({
	store,
	recent,
}: PopoverFeedOptions): PopoverFeed => {
	if (!isDesktopApp() || !supportsToastWindow()) {
		return { refresh: () => {}, stop: () => {} };
	}

	let stopped = false;
	let running = false;
	let queued = false;
	let debounce: ReturnType<typeof setTimeout> | null = null;
	let lastPayload = '';
	let unlisten: (() => void) | null = null;
	const seenLimits = new Set<string>();

	const push = async (extras: PopoverExtras) => {
		const payload = JSON.stringify(extras);
		if (payload === lastPayload) return;
		const { invoke } = await import('@tauri-apps/api/core');
		await invoke('popover_extras_update', { extras });
		lastPayload = payload;
	};

	const cycle = async (): Promise<void> => {
		if (running) {
			queued = true;
			return;
		}
		running = true;
		try {
			const extras = await gatherExtras(store, recent);
			if (!stopped) {
				await push(extras);
				for (const { key, toast } of limitToasts(
					extras.usage,
					seenLimits,
					Date.now() / 1000,
				)) {
					if (await pushToast(toast)) seenLimits.add(key);
				}
			}
		} catch (error) {
			console.error('popover feed update failed', error);
		} finally {
			running = false;
		}
		if (queued && !stopped) {
			queued = false;
			void cycle();
		}
	};

	const refresh = () => {
		if (stopped) return;
		if (debounce) clearTimeout(debounce);
		debounce = setTimeout(() => {
			debounce = null;
			void cycle();
		}, DEBOUNCE_MS);
	};

	const interval = setInterval(refresh, REFRESH_MS);
	window.addEventListener('focus', refresh);
	import('@tauri-apps/api/event')
		.then(({ listen }) => listen('popover://show', refresh))
		.then((stop) => {
			if (stopped) stop();
			else unlisten = stop;
		})
		.catch((error) => console.error('popover shown listener failed', error));
	refresh();

	return {
		refresh,
		stop: () => {
			stopped = true;
			clearInterval(interval);
			if (debounce) clearTimeout(debounce);
			window.removeEventListener('focus', refresh);
			unlisten?.();
		},
	};
};
