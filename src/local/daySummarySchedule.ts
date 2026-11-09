export interface ScheduleSettings {
	enabled: boolean;
	time: string;
	lastRunDay: string | null;
	enabledAt: number | null;
}

export type ScheduleDecision =
	{ kind: 'idle' } | { kind: 'due-today' } | { kind: 'missed'; day: Date };

export interface ScheduleStorage {
	getItem(key: string): string | null;
	setItem(key: string, value: string): void;
}

export const STORAGE_KEY = 'tmgr:day-summary-schedule';

export const DEFAULT_SCHEDULE: ScheduleSettings = {
	enabled: false,
	time: '18:00',
	lastRunDay: null,
	enabledAt: null,
};

const pad = (n: number) => String(n).padStart(2, '0');

export const localDay = (date: Date): string =>
	`${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())}`;

const parseTime = (time: string): [number, number] | null => {
	const match = /^(\d{2}):(\d{2})$/.exec(time);
	if (!match) return null;
	const [hours, minutes] = [Number(match[1]), Number(match[2])];
	return hours < 24 && minutes < 60 ? [hours, minutes] : null;
};

const scheduledAt = (day: Date, time: string): Date | null => {
	const parsed = parseTime(time);
	return parsed
		? new Date(day.getFullYear(), day.getMonth(), day.getDate(), ...parsed)
		: null;
};

export const decide = ({
	now,
	settings,
	startup = false,
}: {
	now: Date;
	settings: ScheduleSettings;
	startup?: boolean;
}): ScheduleDecision => {
	const idle: ScheduleDecision = { kind: 'idle' };
	if (!settings.enabled) return idle;
	const today = localDay(now);
	const last = settings.lastRunDay;
	if (last === today) return idle;
	const todayAt = scheduledAt(now, settings.time);
	if (!todayAt) return idle;
	if (now >= todayAt) return { kind: 'due-today' };
	if (!startup) return idle;
	const yesterday = new Date(
		now.getFullYear(),
		now.getMonth(),
		now.getDate() - 1,
		12,
	);
	const yesterdayAt = scheduledAt(yesterday, settings.time);
	if (!yesterdayAt || (last !== null && last >= localDay(yesterday)))
		return idle;
	if (settings.enabledAt === null || settings.enabledAt > yesterdayAt.getTime())
		return idle;
	return { kind: 'missed', day: yesterday };
};

export const applyChange = (
	current: ScheduleSettings,
	change: Partial<Pick<ScheduleSettings, 'enabled' | 'time'>>,
	now: Date,
): ScheduleSettings => {
	const next = { ...current, ...change };
	const turnedOn = !current.enabled && next.enabled;
	const timeChanged = next.time !== current.time;
	if (turnedOn) next.enabledAt = now.getTime();
	if (turnedOn || timeChanged) {
		const at = scheduledAt(now, next.time);
		const today = localDay(now);
		if (at && now >= at) next.lastRunDay = today;
		else if (next.lastRunDay === today) next.lastRunDay = null;
	}
	return next;
};

export const loadSchedule = (storage: ScheduleStorage): ScheduleSettings => {
	try {
		const raw = JSON.parse(storage.getItem(STORAGE_KEY) ?? 'null');
		if (
			raw &&
			typeof raw.enabled === 'boolean' &&
			typeof raw.time === 'string' &&
			parseTime(raw.time)
		) {
			return {
				enabled: raw.enabled,
				time: raw.time,
				lastRunDay: typeof raw.lastRunDay === 'string' ? raw.lastRunDay : null,
				enabledAt: typeof raw.enabledAt === 'number' ? raw.enabledAt : null,
			};
		}
	} catch {
		return { ...DEFAULT_SCHEDULE };
	}
	return { ...DEFAULT_SCHEDULE };
};

export const saveSchedule = (
	storage: ScheduleStorage,
	settings: ScheduleSettings,
): void => {
	try {
		storage.setItem(STORAGE_KEY, JSON.stringify(settings));
	} catch {
		return;
	}
};
