export interface LocalPattern {
	frequency: string;
	interval: number | null;
	days_of_week: string[];
	day_of_frequency: number | null;
	month: number | null;
	start_at: string | null;
	end_at: string | null;
	occurrences: number | null;
	scheduled_time: string | null;
	duration_min: number | null;
	reminder_min: number | null;
}

export interface LocalRoutine {
	id: number;
	title: string;
	description: string | null;
	category: string | null;
	approximatelyTime: number | null;
	pattern: LocalPattern | null;
	scheduledDate: string | null;
	scheduledTime: string | null;
	hasHistory: boolean;
}

export interface TaskTarget {
	workspaceId: number;
	categoryId: number | null;
	statusId: number;
}

export interface RoutineRequest {
	path: string;
	body: Record<string, any>;
}

const timeOf = (
	value: string | null | undefined,
): { hours: number; minutes: number } | null => {
	const match = /^(\d{1,2}):(\d{2})/.exec(value ?? '');
	if (!match) return null;
	const hours = Number(match[1]);
	const minutes = Number(match[2]);
	return hours === 0 && minutes === 0 ? null : { hours, minutes };
};

const defined = (fields: Record<string, any>): Record<string, any> =>
	Object.fromEntries(
		Object.entries(fields).filter(([, value]) => value != null),
	);

const recurrenceOf = (pattern: LocalPattern) =>
	defined({
		frequency: pattern.frequency,
		interval: pattern.interval,
		days_of_week: pattern.days_of_week?.length ? pattern.days_of_week : null,
		day_of_frequency: pattern.day_of_frequency,
		month: pattern.month,
		time: timeOf(pattern.scheduled_time),
		duration_min: pattern.duration_min,
		reminder_min: pattern.reminder_min,
		dtstart: pattern.start_at,
		dtend: pattern.end_at,
	});

export const buildRoutineRequest = (
	routine: LocalRoutine,
	workspaceId: number,
): RoutineRequest => {
	const time =
		timeOf(routine.scheduledTime) ?? timeOf(routine.pattern?.scheduled_time);
	const dated = !!routine.scheduledDate || !!routine.pattern;
	return {
		path: dated ? 'daily-routines/tasks/recurring' : 'daily-routines/tasks',
		body: defined({
			title: routine.title,
			description: routine.description || null,
			workspace_id: workspaceId,
			approximately_time: routine.approximatelyTime,
			scheduled_date: routine.scheduledDate,
			scheduled_time: routine.scheduledDate || routine.pattern ? time : null,
			recurrence: routine.pattern ? recurrenceOf(routine.pattern) : null,
		}),
	};
};

export const buildTaskRequest = (
	routine: LocalRoutine,
	target: TaskTarget,
): Record<string, any> =>
	defined({
		title: routine.title,
		description: routine.description || null,
		workspace_id: target.workspaceId,
		status_id: target.statusId,
		project_category_id: target.categoryId,
	});
