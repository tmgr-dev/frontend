export type Frequency = 'NONE' | 'DAILY' | 'WEEKLY' | 'MONTHLY' | 'YEARLY';

export interface RecurrencePattern {
	frequency: Frequency | string | null;
	interval: number;
	day_of_frequency: number | null;
	month: number | null;
	days_of_week: string[];
	start_date: string | null;
	end_date: string | null;
	occurrences: number | null;
	scheduled_time: string | null;
	duration_min: number | null;
	reminder_min: number | null;
}

export interface RoutineCategoryJson {
	id: string;
	name: string;
	color: string;
}

const DEFAULT_TIME = '09:00:00';
const DOW_KEYS = ['SUN', 'MON', 'TUE', 'WED', 'THU', 'FRI', 'SAT'];

const pad2 = (n: number): string => String(n).padStart(2, '0');

const parseYmd = (day: string): { y: number; m: number; d: number } => {
	const [y, m, d] = day.split('-').map(Number);
	return { y, m, d };
};

const toUtcDate = (day: string): Date => {
	const { y, m, d } = parseYmd(day);
	return new Date(Date.UTC(y, m - 1, d));
};

const fromUtcDate = (dt: Date): string =>
	`${dt.getUTCFullYear()}-${pad2(dt.getUTCMonth() + 1)}-${pad2(
		dt.getUTCDate(),
	)}`;

const dowKeyOf = (dt: Date): string => DOW_KEYS[dt.getUTCDay()];

const startOfWeekUtc = (dt: Date): Date => {
	const day = dt.getUTCDay();
	const diff = day === 0 ? 6 : day - 1;
	const out = new Date(dt);
	out.setUTCDate(out.getUTCDate() - diff);
	return out;
};

const monthsBetween = (from: string, to: string): number => {
	const a = parseYmd(from);
	const b = parseYmd(to);
	return (b.y - a.y) * 12 + (b.m - a.m);
};

export const addDays = (day: string, days: number): string => {
	const dt = toUtcDate(day);
	dt.setUTCDate(dt.getUTCDate() + days);
	return fromUtcDate(dt);
};

export const daysBetween = (from: string, to: string): number => {
	const a = toUtcDate(from).getTime();
	const b = toUtcDate(to).getTime();
	return Math.round((b - a) / 86400000);
};

export const localDate = (now: Date): string =>
	`${now.getFullYear()}-${pad2(now.getMonth() + 1)}-${pad2(now.getDate())}`;

export const localDateTime = (now: Date): string =>
	`${localDate(now)}T${pad2(now.getHours())}:${pad2(now.getMinutes())}:${pad2(
		now.getSeconds(),
	)}`;

// ── datetime string helpers (naive 'YYYY-MM-DDTHH:MM:SS', UTC-based Date math) ──

const dateOf = (dt: string): string => dt.split('T')[0];

const combineDateTime = (date: string, time: string): string =>
	`${date}T${time}`;

const toUtcDateTime = (dt: string): Date => {
	const [datePart, timePart] = dt.split('T');
	const { y, m, d } = parseYmd(datePart);
	const [hh, mm, ss] = (timePart ?? '00:00:00').split(':').map(Number);
	return new Date(Date.UTC(y, m - 1, d, hh, mm, ss || 0));
};

const fromUtcDateTime = (dt: Date): string =>
	`${fromUtcDate(dt)}T${pad2(dt.getUTCHours())}:${pad2(
		dt.getUTCMinutes(),
	)}:${pad2(dt.getUTCSeconds())}`;

const addDaysToDateTime = (dt: string, n: number): string => {
	const [date, time] = dt.split('T');
	return combineDateTime(addDays(date, n), time);
};

/** Mirrors java.time's plusMonths/plusYears clamp: an invalid day-of-month rolls back to the last valid day. */
const addMonthsClamped = (dt: Date, months: number): Date => {
	const y = dt.getUTCFullYear();
	const m = dt.getUTCMonth();
	const d = dt.getUTCDate();
	const totalMonths = y * 12 + m + months;
	const newY = Math.floor(totalMonths / 12);
	const newM = totalMonths - newY * 12;
	const daysInNewMonth = new Date(Date.UTC(newY, newM + 1, 0)).getUTCDate();
	const out = new Date(dt);
	out.setUTCFullYear(newY, newM, Math.min(d, daysInNewMonth));
	return out;
};

const addMonthsToDateTime = (dt: string, months: number): string =>
	fromUtcDateTime(addMonthsClamped(toUtcDateTime(dt), months));

const addHoursToDateTime = (dt: string, hours: number): string => {
	const d = toUtcDateTime(dt);
	d.setUTCHours(d.getUTCHours() + hours);
	return fromUtcDateTime(d);
};

// ── firesOn / expandDates — port of RoutineExpansionService ─────────────────

export const firesOn = (
	p: RecurrencePattern,
	day: string,
	rangeFrom: string,
): boolean => {
	if (p.start_date != null && day < p.start_date) return false;
	if (p.end_date != null && day > p.end_date) return false;

	const startDate = p.start_date ?? rangeFrom;
	const interval = Math.max(1, p.interval ?? 1);
	const freq = (p.frequency ?? '').toString().toUpperCase();

	switch (freq) {
		case 'DAILY': {
			const diff = daysBetween(startDate, day);
			return diff >= 0 && diff % interval === 0;
		}
		case 'WEEKLY': {
			if (p.days_of_week.length === 0) return false;
			const dow = dowKeyOf(toUtcDate(day));
			if (!p.days_of_week.includes(dow)) return false;
			const sowStart = fromUtcDate(startOfWeekUtc(toUtcDate(startDate)));
			const sowDay = fromUtcDate(startOfWeekUtc(toUtcDate(day)));
			const weeksDiff = Math.round(daysBetween(sowStart, sowDay) / 7);
			return weeksDiff >= 0 && weeksDiff % interval === 0;
		}
		case 'MONTHLY': {
			const dof = p.day_of_frequency;
			if (dof == null || parseYmd(day).d !== dof) return false;
			const monthsDiff = monthsBetween(startDate, day);
			return monthsDiff >= 0 && monthsDiff % interval === 0;
		}
		case 'YEARLY': {
			const dof = p.day_of_frequency;
			const mon = p.month;
			if (dof == null || mon == null) return false;
			const { y, m, d } = parseYmd(day);
			if (d !== dof) return false;
			if (m !== mon + 1) return false;
			const yearsDiff = y - parseYmd(startDate).y;
			return yearsDiff >= 0 && yearsDiff % interval === 0;
		}
		default:
			return false;
	}
};

export const expandDates = (
	p: RecurrencePattern,
	from: string,
	to: string,
): string[] => {
	const dates: string[] = [];
	let cursor = from;
	while (cursor <= to) {
		if (firesOn(p, cursor, from)) dates.push(cursor);
		cursor = addDays(cursor, 1);
	}
	return dates;
};

// ── nextInstance — port of NextInstanceCalculator ────────────────────────────

const advanceToMatchingDay = (cursor: string, days: Set<string>): string => {
	let c = cursor;
	for (let i = 0; i < 14; i++) {
		if (days.has(dowKeyOf(toUtcDate(c)))) return c;
		c = addDays(c, 1);
	}
	return c;
};

const weeklyNext = (
	last: string | null,
	now: string,
	time: string,
	interval: number,
	days: Set<string>,
): string => {
	let cursor = advanceToMatchingDay(
		last != null ? addDays(dateOf(last), 1) : dateOf(now),
		days,
	);
	let candidate = combineDateTime(cursor, time);
	if (last != null && candidate <= last) {
		cursor = advanceToMatchingDay(addDays(cursor, interval * 7), days);
		candidate = combineDateTime(cursor, time);
	}
	return candidate;
};

export const nextInstance = (
	p: RecurrencePattern,
	last: string | null,
	now: string,
): string | null => {
	const freq = (p.frequency ?? '').toString().toUpperCase();
	const interval = Math.max(1, p.interval ?? 1);
	const time = p.scheduled_time ?? DEFAULT_TIME;
	const startAt =
		p.start_date != null ? combineDateTime(p.start_date, '00:00:00') : null;
	const endAt =
		p.end_date != null ? combineDateTime(p.end_date, '00:00:00') : null;

	if (startAt != null && startAt > now) return null;

	const days = new Set(p.days_of_week.map((d) => d.toUpperCase()));

	let next: string;
	if (freq === 'WEEKLY' && days.size > 0) {
		next = weeklyNext(last, now, time, interval, days);
	} else if (last == null) {
		const todayAtTime = combineDateTime(dateOf(now), time);
		next = now < todayAtTime ? todayAtTime : addDaysToDateTime(todayAtTime, 1);
	} else {
		let base: string | null;
		switch (freq) {
			case 'DAILY':
				base = addDaysToDateTime(last, interval);
				break;
			case 'WEEKLY':
				base = addDaysToDateTime(last, interval * 7);
				break;
			case 'MONTHLY':
				base = addMonthsToDateTime(last, interval);
				break;
			case 'YEARLY':
				base = addMonthsToDateTime(last, interval * 12);
				break;
			default:
				base = null;
		}
		if (base == null) return null;
		next = combineDateTime(dateOf(base), time);
	}

	if (startAt != null && next < startAt) {
		next = combineDateTime(dateOf(startAt), time);
	}
	if (endAt != null && next > endAt) return null;
	if (next > addHoursToDateTime(now, 24)) return null;

	return next;
};

// ── normalizeRecurrence — port of PatternRequest/RoutineTaskRequest/UpdateRoutineRequest ──

const toIntOrNull = (v: unknown): number | null =>
	typeof v === 'number' && Number.isFinite(v) ? v : null;

const normalizeDateInput = (v: unknown): string | null => {
	if (typeof v !== 'string' || v.length === 0) return null;
	return v.slice(0, 10);
};

const normalizeDaysOfWeek = (v: unknown): string[] => {
	if (!Array.isArray(v)) return [];
	return v
		.filter((x): x is string => typeof x === 'string')
		.map((s) => s.toUpperCase());
};

const resolveScheduledTime = (v: unknown): string | null => {
	if (v != null && typeof v === 'object' && !Array.isArray(v)) {
		const o = v as Record<string, unknown>;
		const h = o.hours;
		const min = o.minutes;
		if (typeof h === 'number' && typeof min === 'number') {
			const sec = typeof o.seconds === 'number' ? o.seconds : 0;
			return `${pad2(h)}:${pad2(min)}:${pad2(sec)}`;
		}
		return null;
	}
	if (typeof v === 'string') {
		const m = /^(\d{1,2}):(\d{1,2})(?::(\d{1,2}))?$/.exec(v.trim());
		if (!m) return null;
		const h = Number(m[1]);
		const min = Number(m[2]);
		const sec = m[3] != null ? Number(m[3]) : 0;
		return `${pad2(h)}:${pad2(min)}:${pad2(sec)}`;
	}
	return null;
};

export const normalizeRecurrence = (raw: unknown): RecurrencePattern | null => {
	if (raw == null || typeof raw !== 'object') return null;
	const r = raw as Record<string, unknown>;

	const freqRaw = r.frequency;
	const freq = typeof freqRaw === 'string' ? freqRaw.toUpperCase() : '';
	if (freq === '' || freq === 'NONE') return null;

	const interval =
		typeof r.interval === 'number' && r.interval > 0 ? r.interval : 1;

	return {
		frequency: freq,
		interval,
		day_of_frequency: toIntOrNull(r.day_of_frequency ?? r.dayOfFrequency),
		month: toIntOrNull(r.month),
		days_of_week: normalizeDaysOfWeek(r.days_of_week ?? r.daysOfWeek),
		start_date: normalizeDateInput(r.start_date ?? r.start_at ?? r.dtstart),
		end_date: normalizeDateInput(r.end_date ?? r.end_at ?? r.dtend),
		occurrences: toIntOrNull(r.occurrences),
		scheduled_time: resolveScheduledTime(
			r.time ?? r.scheduled_time ?? r.scheduledTime,
		),
		duration_min: toIntOrNull(r.duration_min ?? r.durationMin),
		reminder_min: toIntOrNull(r.reminder_min ?? r.reminderMin),
	};
};

// ── resolveCategory — mirror of categoryMap.ts / RoutineExpansionService.resolveCategory ──

const ROUTINE_CATEGORIES: Record<string, RoutineCategoryJson> = {
	work: { id: 'work', name: 'Work', color: '#5b8cff' },
	health: { id: 'health', name: 'Health', color: '#22c55e' },
	learn: { id: 'learn', name: 'Learn', color: '#a78bfa' },
	home: { id: 'home', name: 'Home', color: '#f5b54a' },
	social: { id: 'social', name: 'Social', color: '#ec4899' },
	none: { id: 'none', name: 'General', color: '#888888' },
};

export const resolveCategory = (
	id: string | null | undefined,
): RoutineCategoryJson => {
	const key = id == null || id.trim() === '' ? 'none' : id;
	return ROUTINE_CATEGORIES[key] ?? ROUTINE_CATEGORIES.none;
};
