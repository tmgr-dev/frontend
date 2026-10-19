import type { StatusLike, TaskLike } from '@/utils/simpleStatus';

const HIDDEN_FROM_FILTER = ['archived', 'hidden'];

export const filterableStatuses = <T extends StatusLike>(
	statuses: T[] | null | undefined,
): T[] =>
	(statuses || []).filter(
		(status) => !HIDDEN_FROM_FILTER.includes(String(status.type)),
	);

export const matchesStatusIds = (
	task: TaskLike,
	statusIds: number[],
): boolean =>
	statusIds.length === 0 ||
	(task.status_id != null && statusIds.includes(Number(task.status_id)));

export const filterTasksByStatusIds = <T extends TaskLike>(
	tasks: T[],
	statusIds: number[],
): T[] =>
	statusIds.length === 0
		? tasks
		: tasks.filter((task) => matchesStatusIds(task, statusIds));
