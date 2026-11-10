import { parseJson } from '../serialize';
import { addDays, daysBetween, localDate } from './recurrence';

export interface Version {
	segmentId: number | null;
	validFrom: string | null;
	validTo: string | null;
	anchor: string | null;
	recurring: boolean;
	frequency: string | null;
	interval: number | null;
	dayOfFrequency: number | null;
	month: number | null;
	daysOfWeek: string[];
	scheduledTime: string | null;
	durationMin: number | null;
	reminderMin: number | null;
	title: string | null;
	description: string | null;
	routineCategory: string | null;
}

const DOW_KEYS = ['SUN', 'MON', 'TUE', 'WED', 'THU', 'FRI', 'SAT'];

const DOW_CODES: Record<string, string> = {
	MO: 'MON',
	TU: 'TUE',
	WE: 'WED',
	TH: 'THU',
	FR: 'FRI',
	SA: 'SAT',
	SU: 'SUN',
};

export const normalizeDays = (days: unknown): string[] => {
	if (!Array.isArray(days)) return [];
	const out = new Set<string>();
	for (const d of days) {
		if (typeof d !== 'string') continue;
		const key = d.trim().toUpperCase();
		out.add(DOW_CODES[key] ?? key);
	}
	return [...out];
};

export const isLive = (v: Version): boolean => v.segmentId == null;

export const withTime = (v: Version, time: string | null): Version => ({
	...v,
	scheduledTime: time,
});

export const covers = (v: Version, day: string): boolean =>
	(v.validFrom == null || day >= v.validFrom) &&
	(v.validTo == null || day <= v.validTo);

/** The last day governed by a frozen segment, or null when the routine has no history. */
export const frozenUntil = (segments: Version[]): string | null => {
	let max: string | null = null;
	for (const s of segments) {
		if (s.validTo != null && (max == null || s.validTo > max)) max = s.validTo;
	}
	return max;
};

/** The version governing `day`; null when no version does (a gap, or no live pattern). */
export const governing = (
	segments: Version[],
	live: Version | null,
	day: string,
): Version | null => {
	const frozen = frozenUntil(segments);
	if (frozen != null && day <= frozen) {
		for (const s of segments) {
			if (covers(s, day)) return s;
		}
		return null;
	}
	return live;
};

const utc = (day: string): Date => {
	const [y, m, d] = day.split('-').map(Number);
	return new Date(Date.UTC(y, m - 1, d));
};

const startOfWeek = (day: string): string => {
	const dow = utc(day).getUTCDay();
	return addDays(day, dow === 0 ? -6 : 1 - dow);
};

const monthsBetween = (a: string, b: string): number => {
	const [ay, am] = a.split('-').map(Number);
	const [by, bm] = b.split('-').map(Number);
	return (by - ay) * 12 + (bm - am);
};

/** Whether `v` produces an occurrence on `day`; `fallbackAnchor` is used when v has no anchor. */
export const firesOn = (
	v: Version,
	day: string,
	fallbackAnchor: string | null,
): boolean => {
	if (!v.recurring || !covers(v, day)) return false;
	const anchor = v.anchor ?? v.validFrom ?? fallbackAnchor;
	if (anchor == null) return false;
	const interval = Math.max(1, v.interval ?? 1);
	const freq = (v.frequency ?? '').toUpperCase();
	switch (freq) {
		case 'DAILY': {
			const diff = daysBetween(anchor, day);
			return diff >= 0 && diff % interval === 0;
		}
		case 'WEEKLY': {
			if (!v.daysOfWeek.includes(DOW_KEYS[utc(day).getUTCDay()])) return false;
			const weeks = Math.round(
				daysBetween(startOfWeek(anchor), startOfWeek(day)) / 7,
			);
			return weeks >= 0 && weeks % interval === 0;
		}
		case 'MONTHLY': {
			if (
				v.dayOfFrequency == null ||
				Number(day.slice(8, 10)) !== v.dayOfFrequency
			)
				return false;
			const months = monthsBetween(anchor, day);
			return months >= 0 && months % interval === 0;
		}
		case 'YEARLY': {
			if (v.dayOfFrequency == null || v.month == null) return false;
			if (Number(day.slice(8, 10)) !== v.dayOfFrequency) return false;
			if (Number(day.slice(5, 7)) !== v.month + 1) return false;
			const years = Number(day.slice(0, 4)) - Number(anchor.slice(0, 4));
			return years >= 0 && years % interval === 0;
		}
		default:
			return false;
	}
};

export const fromSegment = (row: any): Version => ({
	segmentId: Number(row.id),
	validFrom: row.valid_from ?? null,
	validTo: row.valid_to ?? null,
	anchor: row.anchor_date ?? null,
	recurring: row.is_recurring == null ? true : Number(row.is_recurring) !== 0,
	frequency: row.frequency ?? null,
	interval: row.interval ?? null,
	dayOfFrequency: row.day_of_frequency ?? null,
	month: row.month ?? null,
	daysOfWeek: parseJson<string[]>(row.days_of_week, []),
	scheduledTime: row.scheduled_time ?? null,
	durationMin: row.duration_min ?? null,
	reminderMin: row.reminder_min ?? null,
	title: row.title ?? null,
	description: row.description ?? null,
	routineCategory: row.routine_category ?? null,
});

export const createdOn = (routine: any, timeZone?: string): string | null =>
	routine?.created_at
		? localDate(new Date(routine.created_at), timeZone)
		: null;

const day10 = (value: any): string | null =>
	value ? String(value).slice(0, 10) : null;

/** The live routine + pattern as a version; `clampToStart` is the expansion variant (valid from the pattern's own start too). */
export const liveVersion = (
	routine: any,
	pattern: any,
	segments: Version[],
	clampToStart: boolean,
	timeZone?: string,
): Version => {
	const frozen = frozenUntil(segments);
	const start = day10(pattern.start_at);
	let validFrom = frozen == null ? null : addDays(frozen, 1);
	if (clampToStart && start != null && (validFrom == null || start > validFrom))
		validFrom = start;
	const anchor =
		start == null && segments.length ? createdOn(routine, timeZone) : start;
	return {
		segmentId: null,
		validFrom,
		validTo: day10(pattern.end_at),
		anchor,
		recurring: true,
		frequency: pattern.frequency ?? null,
		interval: pattern.interval ?? null,
		dayOfFrequency: pattern.day_of_frequency ?? null,
		month: pattern.month ?? null,
		daysOfWeek: parseJson<string[]>(pattern.days_of_week, []),
		scheduledTime: pattern.scheduled_time ?? null,
		durationMin: routine.approximately_time ?? pattern.duration_min ?? null,
		reminderMin: pattern.reminder_min ?? null,
		title: routine.title ?? null,
		description: routine.description ?? null,
		routineCategory: routine.routine_category ?? null,
	};
};
