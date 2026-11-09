import { toast, ToastAction } from '@/components/ui/toast';
import { t } from '@/i18n';
import {
	applyChange,
	decide,
	loadSchedule,
	localDay,
	saveSchedule,
	type ScheduleSettings,
} from '@/local/daySummarySchedule';
import { h, ref } from 'vue';

const TICK_MS = 60_000;
const CHANGED_EVENT = 'tmgr:day-summary-schedule-changed';

export const daySummaryRequest = ref<Date | null>(null);

export const readSchedule = (): ScheduleSettings => loadSchedule(localStorage);

export const updateSchedule = (
	change: Partial<Pick<ScheduleSettings, 'enabled' | 'time'>>,
	now: Date = new Date(),
): ScheduleSettings => {
	const next = applyChange(readSchedule(), change, now);
	saveSchedule(localStorage, next);
	window.dispatchEvent(new Event(CHANGED_EVENT));
	return next;
};

const notifyNative = async () => {
	const { invoke } = await import('@tauri-apps/api/core');
	await invoke('plugin_notify', {
		title: t('desktop.daySummarySchedule.readyTitle'),
		body: t('desktop.daySummarySchedule.readyBody'),
		token: null,
	}).catch(() => undefined);
};

const showToast = (title: string, description: string, day: Date | null) => {
	toast({
		title,
		description,
		action: h(
			ToastAction,
			{
				altText: t('desktop.daySummarySchedule.open'),
				onClick: () => {
					daySummaryRequest.value = day ?? new Date();
				},
			},
			() => t('desktop.daySummarySchedule.open'),
		),
	});
};

const markRun = (day: Date) => {
	saveSchedule(localStorage, {
		...readSchedule(),
		lastRunDay: localDay(day),
	});
};

export const startDaySummarySchedule = (): (() => void) => {
	let timer: ReturnType<typeof setInterval> | null = null;

	const tick = (startup = false) => {
		const now = new Date();
		const decision = decide({ now, settings: readSchedule(), startup });
		if (decision.kind === 'due-today') {
			markRun(now);
			showToast(
				t('desktop.daySummarySchedule.readyTitle'),
				t('desktop.daySummarySchedule.readyBody'),
				null,
			);
			if (!document.hasFocus()) void notifyNative();
		} else if (decision.kind === 'missed') {
			markRun(decision.day);
			showToast(
				t('desktop.daySummarySchedule.missedTitle'),
				t('desktop.daySummarySchedule.missedBody'),
				decision.day,
			);
		}
	};

	const sync = (startup = false) => {
		const enabled = readSchedule().enabled;
		if (!enabled && timer !== null) {
			clearInterval(timer);
			timer = null;
		}
		if (!enabled) return;
		if (timer === null) timer = setInterval(tick, TICK_MS);
		tick(startup);
	};

	const onChanged = () => sync();
	window.addEventListener(CHANGED_EVENT, onChanged);
	sync(true);

	return () => {
		window.removeEventListener(CHANGED_EVENT, onChanged);
		if (timer !== null) clearInterval(timer);
		timer = null;
	};
};
