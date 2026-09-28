

export type RoutineStatus = 'PENDING' | 'COMPLETED' | 'SKIPPED';

export interface RoutineEntry {
	routineId: number;
	instanceId: number | null;
	title: string;
	description: string | null;
	date: string;
	time: string | null;
	status: RoutineStatus;
	completed: boolean;
	recurring: boolean;
	frequency: string | null;
	virtual: boolean;
}

export interface Routine {
	id: number;
	title: string;
	description: string | null;
	scheduledDate: string | null;
	scheduledTime: string | null;
	createdAt: string;
	updatedAt: string;
}

export interface RoutineInstance {
	id: number;
	routineId: number;
	date: string;
	time: string | null;
	status: RoutineStatus;
}

const displayTime = (value: unknown): string | null => {
	const time = value == null ? null : String(value);
	return time && time !== '00:00:00' ? time.slice(0, 5) : null;
};

/** Accepts both `routineJson` output and a raw `routines` table row: both share these column names. */
export const toRoutine = (row: any): Routine => ({
	id: Number(row.id),
	title: String(row.title),
	description: row.description ?? null,
	scheduledDate: row.scheduled_date ?? null,
	scheduledTime: row.scheduled_time ?? null,
	createdAt: row.created_at,
	updatedAt: row.updated_at,
});

/** Accepts `instanceJson` output, a raw `routine_instances` row, or the `complete-on` response shape. */
export const toRoutineInstance = (row: any): RoutineInstance => {
	const scheduledFor = row.scheduled_for != null ? String(row.scheduled_for) : null;
	return {
		id: Number(row.id),
		routineId: Number(row.routine_id ?? row.task_id),
		date: row.scheduled_date ?? scheduledFor?.slice(0, 10) ?? '',
		time: displayTime(row.scheduled_time ?? scheduledFor?.slice(11, 19)),
		status: row.status,
	};
};

/** Accepts a `RoutineEntryJson` from `GET daily-routines/expand`. */
export const toRoutineEntry = (entry: any): RoutineEntry => {
	const frequency = entry.frequency && entry.frequency !== 'NONE' ? entry.frequency : null;
	return {
		routineId: Number(entry.task_id),
		instanceId: entry.instance_id == null ? null : Number(entry.instance_id),
		title: entry.title,
		description: entry.description ?? null,
		date: entry.date,
		time: entry.time ?? null,
		status: String(entry.status).toUpperCase() as RoutineStatus,
		completed: !!entry.completed,
		recurring: frequency !== null,
		frequency,
		virtual: !!entry.virtual,
	};
};
