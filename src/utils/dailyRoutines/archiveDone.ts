import type { RoutineEntry } from '@/types/dailyRoutine';

export function doneUnscheduledIds(
	entries: Pick<RoutineEntry, 'task_id' | 'time' | 'completed' | 'frequency'>[],
): number[] {
	return [
		...new Set(
			entries
				.filter(
					(e) =>
						!e.time && e.completed && (!e.frequency || e.frequency === 'NONE'),
				)
				.map((e) => e.task_id),
		),
	];
}
