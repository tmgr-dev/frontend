import {
	isTaskDone,
	type StatusLike,
	type TaskLike,
} from '@/utils/simpleStatus';

export type TaskStateFilter = 'all' | 'open' | 'done';

export const filterTasksByState = <T extends TaskLike>(
	tasks: T[],
	filter: TaskStateFilter,
	statuses: StatusLike[] | null | undefined,
): T[] => {
	if (filter === 'all') return tasks;
	return tasks.filter(
		(task) => isTaskDone(task, statuses) === (filter === 'done'),
	);
};

export const paginateLocally = <T>(
	items: T[],
	page: number,
	perPage: number,
) => {
	const total = items.length;
	const size = Math.max(1, perPage);
	const lastPage = total === 0 ? 1 : Math.ceil(total / size);
	const current = Math.min(Math.max(1, page), lastPage);
	const start = (current - 1) * size;
	const end = Math.min(total, start + size);
	return {
		data: items.slice(start, end),
		meta: {
			current_page: current,
			per_page: size,
			total,
			last_page: lastPage,
			from: total === 0 ? 0 : start + 1,
			to: end,
		},
	};
};
