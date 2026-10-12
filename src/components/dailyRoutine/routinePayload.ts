import type {
	RoutineCategoryId,
	RoutineEntry,
	RoutineFrequency,
} from '@/types/dailyRoutine';
import {
	isUnscheduledDraft,
	parseTimeStr,
} from '@/utils/dailyRoutines/routineDraft';

export interface RoutineDraft {
	id: number | null;
	title: string;
	description: string;
	cat: RoutineCategoryId;
	timeH: number;
	timeM: number;
	durationMin: number;
	frequency: RoutineFrequency;
	interval: number | null;
	daysOfWeek: number[];
	dayOfMonth: number;
	month: number;
	reminderMin: number | null;
	unscheduled: boolean;
	scheduledDate: string;
}

const DOW_KEYS = ['MON', 'TUE', 'WED', 'THU', 'FRI', 'SAT', 'SUN'];

function dowsToIndices(arr: any[] | null | undefined): number[] {
	if (!arr) return [];
	return arr
		.map((d: any) =>
			typeof d === 'number' ? d : DOW_KEYS.indexOf(String(d).toUpperCase()),
		)
		.filter((i: number) => i >= 0)
		.sort();
}

function parseIsoDate(iso: string): Date {
	const [y, m, d] = iso.split('-').map(Number);
	return new Date(y, m - 1, d);
}

/** Monday-based index (0 = MON) of an ISO date. */
function dowIndex(iso: string): number {
	return (parseIsoDate(iso).getDay() + 6) % 7;
}

/**
 * The Mon–Sun week that contains `iso` and the four weeks after it: one week
 * misses weekdays before a routine's start (or every other week of interval 2).
 */
export function occurrenceRangeOf(iso: string): { from: string; to: string } {
	const d = parseIsoDate(iso);
	const start = new Date(d);
	start.setDate(d.getDate() - dowIndex(iso));
	const end = new Date(start);
	end.setDate(start.getDate() + 34);
	const fmt = (x: Date) =>
		`${x.getFullYear()}-${String(x.getMonth() + 1).padStart(2, '0')}-${String(
			x.getDate(),
		).padStart(2, '0')}`;
	return { from: fmt(start), to: fmt(end) };
}

/**
 * GET /daily-routines/tasks/{id} returns the bare task: no recurrence pattern,
 * no reminder and the category as a plain string. The calendar entry carries
 * frequency, reminder and category; the pattern details are recovered from the
 * occurrence date (monthly/yearly) and from the routine's occurrences in
 * `occurrenceRangeOf` (weekly).
 */
export function routineForEdit(
	full: any,
	entry: RoutineEntry,
	occurrences: RoutineEntry[] = [],
): any {
	const routine: any = {
		...full,
		scheduled_time: entry.time ?? full.scheduled_time ?? null,
		scheduled_date: entry.date ?? full.scheduled_date ?? null,
		durationMin: entry.duration_min ?? full.approximately_time ?? undefined,
		routine_category: entry.routine_category ?? full.routine_category,
		reminder_min: entry.reminder_min ?? null,
	};
	const frequency = entry.frequency;
	if (!frequency || frequency === 'NONE' || !entry.date) return routine;

	const date = parseIsoDate(entry.date);
	const weekdays = new Set<number>([dowIndex(entry.date)]);
	for (const e of occurrences) {
		if (e.task_id === entry.task_id && e.date) weekdays.add(dowIndex(e.date));
	}
	routine.recurrence = {
		frequency,
		days_of_week:
			frequency === 'WEEKLY'
				? [...weekdays].sort().map((i) => DOW_KEYS[i])
				: [],
		day_of_frequency: ['MONTHLY', 'YEARLY'].includes(frequency)
			? date.getDate()
			: undefined,
		month: frequency === 'YEARLY' ? date.getMonth() : undefined,
		reminder_min: entry.reminder_min ?? null,
	};
	return routine;
}

export function routineDraftFrom(r: any, today: string): RoutineDraft {
	const taskTime = parseTimeStr(r.scheduled_time);
	const category =
		typeof r.routine_category === 'string'
			? r.routine_category
			: r.routine_category?.id;
	return {
		id: r.id ?? null,
		title: r.title ?? '',
		description: r.description ?? '',
		cat: (category ?? r.cat ?? 'none') as RoutineCategoryId,
		timeH: r.time?.h ?? r.recurrence?.time?.hours ?? taskTime?.h ?? 9,
		timeM: r.time?.m ?? r.recurrence?.time?.minutes ?? taskTime?.m ?? 0,
		durationMin: r.recurrence?.duration_min ?? r.durationMin ?? 30,
		frequency: (r.recurrence?.frequency ??
			r.frequency ??
			'NONE') as RoutineFrequency,
		interval: r.recurrence?.interval ?? null,
		daysOfWeek: dowsToIndices(r.recurrence?.days_of_week ?? []),
		dayOfMonth: r.recurrence?.day_of_frequency ?? new Date().getDate(),
		month: r.recurrence?.month ?? 0,
		reminderMin: r.recurrence?.reminder_min ?? r.reminder_min ?? null,
		unscheduled: isUnscheduledDraft(r),
		scheduledDate: r.scheduled_date ?? r._draftDate ?? r.date ?? today,
	};
}

function recurrenceOf(draft: RoutineDraft) {
	return {
		frequency: draft.frequency,
		interval: draft.interval || null,
		days_of_week:
			draft.frequency === 'WEEKLY'
				? [...draft.daysOfWeek].sort().map((i) => DOW_KEYS[i])
				: null,
		day_of_frequency: ['MONTHLY', 'YEARLY'].includes(draft.frequency)
			? draft.dayOfMonth
			: null,
		month: draft.frequency === 'YEARLY' ? draft.month : null,
		time: { hours: draft.timeH, minutes: draft.timeM },
		duration_min: draft.durationMin,
		reminder_min: draft.reminderMin,
	};
}

/**
 * Body for PUT /daily-routines/tasks/{id}. The reminder is stored on the
 * recurrence pattern only, and a non-recurring save removes the pattern, so a
 * reminder is sent for repeating routines only.
 *
 * `original` is the draft an edit started from. Its pattern is rebuilt from a
 * few weeks of occurrences and can be incomplete, so the recurrence is sent
 * only when a recurrence field changed; without it the server keeps the pattern.
 */
export function buildRoutinePayload(
	draft: RoutineDraft,
	today: string,
	original?: RoutineDraft,
): any {
	const isRecurring = draft.frequency !== 'NONE';
	const isUnscheduled = !isRecurring && !!draft.unscheduled;
	const payload: any = {
		title: draft.title,
		description: draft.description,
		routine_category: draft.cat,
		is_daily_routine: true,
		is_recurring: isRecurring,
	};
	if (isRecurring) {
		const recurrence = recurrenceOf(draft);
		if (
			!original ||
			JSON.stringify(recurrence) !== JSON.stringify(recurrenceOf(original))
		)
			payload.recurrence = recurrence;
		// Mirror duration onto the task so it stays the single source of truth
		// (day-view resize writes here too); the pattern's duration_min is legacy.
		payload.approximately_time = draft.durationMin;
	} else if (isUnscheduled) {
		payload.unscheduled = true;
	} else {
		payload.scheduled_date = draft.scheduledDate || today;
		payload.scheduled_time = { hours: draft.timeH, minutes: draft.timeM };
		// Duration of a one-off routine lives on the task (approximately_time),
		// not on a recurrence pattern — send it so the block sizes correctly
		// and the edit modal can read it back.
		payload.approximately_time = draft.durationMin;
	}
	return payload;
}
