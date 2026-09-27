import { parseJson } from '../serialize';
import type { LocalContext } from '../types';
import type { RoutineCategoryJson } from './recurrence';
import type { RoutineEntryJson } from './service';

/** Task-shaped JSON for a routine row, mirroring the Java `Task` entity's Jackson output. */
export const routineJson = (row: any, ctx: LocalContext) => ({
	id: row.id,
	title: row.title,
	description: row.description,
	routine_category: row.routine_category ?? null,
	priority: row.priority ?? 'medium',
	approximately_time: row.approximately_time ?? null,
	scheduled_date: row.scheduled_date ?? null,
	scheduled_time: row.scheduled_time ?? null,
	settings: parseJson(row.settings, []),
	status: null,
	status_id: null,
	order: null,
	project_category_id: null,
	category_tasks_sequence_id: null,
	common_time: 0,
	start_time: 0,
	end_time: null,
	recurring: false,
	is_daily_routine: true,
	daily_routine: true,
	workspace_id: ctx.workspace.id,
	user_id: ctx.user.id,
	user: { id: ctx.user.id, name: ctx.user.name },
	created_at: row.created_at,
	updated_at: row.updated_at,
	deleted_at: row.deleted_at ?? null,
	archived_at: row.archived_at ?? null,
});

/** Mirrors the Java `TaskInstance` entity's Jackson output (id, task_id, scheduled_for, ...). */
export const instanceJson = (row: any) => ({
	id: row.id,
	task_id: row.routine_id,
	scheduled_for: `${row.scheduled_for}Z`,
	status: row.status,
	completed_at: row.completed_at ?? null,
	skipped_at: row.skipped_at ?? null,
	created_at: row.created_at,
	updated_at: row.updated_at,
	scheduled_date: String(row.scheduled_for).slice(0, 10),
	scheduled_time: String(row.scheduled_for).slice(11),
});

/** Mirrors the Java `TaskPattern` entity's Jackson output for the legacy PUT .../pattern endpoint. */
export const patternJson = (row: any) => ({
	id: row.id,
	task_id: row.routine_id,
	frequency: row.frequency,
	interval: row.interval,
	day_of_frequency: row.day_of_frequency ?? null,
	month: row.month ?? null,
	days_of_week: parseJson<string[]>(row.days_of_week, []),
	start_at: row.start_at ?? null,
	end_at: row.end_at ?? null,
	occurrences: row.occurrences ?? null,
	scheduled_time: row.scheduled_time ?? null,
	duration_min: row.duration_min ?? null,
	reminder_min: row.reminder_min ?? null,
	created_at: row.created_at,
	updated_at: row.updated_at,
});

const displayTime = (time: string | null | undefined): string | null =>
	time && time !== '00:00:00' ? time.slice(0, 5) : null;

export const buildPatternEntry = (
	row: any,
	day: string,
	inst: any | null,
	resolveCategory: (id: string | null | undefined) => RoutineCategoryJson,
): RoutineEntryJson => {
	const time: string | null = row.scheduled_time ?? null;
	const status = inst ? String(inst.status).toLowerCase() : 'pending';
	return {
		task_id: row.routine_id,
		title: row.title,
		description: row.description,
		routine_category: resolveCategory(row.routine_category),
		date: day,
		time: displayTime(time),
		scheduled_for: `${day}T${time ?? '00:00:00'}Z`,
		duration_min: row.approximately_time ?? row.duration_min ?? null,
		reminder_min: row.reminder_min ?? null,
		frequency: row.frequency,
		status,
		completed: status === 'completed',
		instance_id: inst ? inst.id : null,
		virtual: !inst,
		created_at: row.created_at ?? null,
		updated_at: row.updated_at ?? null,
	};
};

export const buildOrphanEntry = (
	row: any,
	resolveCategory: (id: string | null | undefined) => RoutineCategoryJson,
): RoutineEntryJson => {
	const [date, timePart] = String(row.scheduled_for).split('T');
	const status = String(row.status ?? 'PENDING').toLowerCase();
	return {
		task_id: row.routine_id,
		title: row.title,
		description: row.description,
		routine_category: resolveCategory(row.routine_category),
		date,
		time: displayTime(timePart ?? null),
		scheduled_for: `${row.scheduled_for}Z`,
		duration_min: row.duration_min ?? null,
		reminder_min: row.reminder_min ?? null,
		frequency: row.pattern_frequency ?? null,
		status,
		completed: status === 'completed',
		instance_id: row.id,
		virtual: false,
		created_at: row.created_at ?? null,
		updated_at: row.updated_at ?? null,
	};
};

export const buildPlainEntry = (
	row: any,
	day: string,
	resolveCategory: (id: string | null | undefined) => RoutineCategoryJson,
): RoutineEntryJson => {
	const time: string | null = row.scheduled_time ?? null;
	return {
		task_id: row.routine_id,
		title: row.title,
		description: row.description,
		routine_category: resolveCategory(row.routine_category),
		date: day,
		time: displayTime(time),
		scheduled_for: `${day}T${time ?? '00:00:00'}Z`,
		duration_min: row.approximately_time ?? null,
		reminder_min: null,
		frequency: 'NONE',
		status: 'pending',
		completed: false,
		instance_id: null,
		virtual: true,
		created_at: row.created_at ?? null,
		updated_at: row.updated_at ?? null,
	};
};
