import type { RoutineEntry } from '@/types/dailyRoutine';

/**
 * Newest-first ordering for unscheduled routine lists.
 *
 * Primary key: created_at (backend emits it for every expansion entry).
 * Fallback: task_id (auto-increment, a stand-in for creation time on
 * entries produced before the backend started emitting created_at).
 * Never returns NaN from the comparator — a single undefined key would
 * otherwise scramble the whole Array.prototype.sort result.
 */
export function sortUnscheduledNewestFirst<
	T extends Pick<RoutineEntry, 'task_id' | 'created_at'>,
>(entries: T[]): T[] {
	return [...entries].sort((a, b) => {
		const at = a.created_at ? Date.parse(a.created_at) : NaN;
		const bt = b.created_at ? Date.parse(b.created_at) : NaN;
		if (!Number.isNaN(at) && !Number.isNaN(bt) && at !== bt) {
			return bt - at;
		}
		return (b.task_id ?? 0) - (a.task_id ?? 0);
	});
}
