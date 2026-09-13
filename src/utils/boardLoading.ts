import { createRequestSequence } from './requestSequence';

export function createBoardLoader<T>(
	fetch: () => Promise<T>,
	apply: (value: T) => void,
	error: (error: unknown) => void,
	busy: (value: boolean) => void,
) {
	const sequence = createRequestSequence();
	let timer: ReturnType<typeof setTimeout> | undefined;
	let disposed = false;
	const run = async () => {
		if (disposed) return;
		clearTimeout(timer);
		const request = sequence.begin();
		busy(true);
		try {
			const value = await fetch();
			if (sequence.isCurrent(request)) apply(value);
		} catch (cause) {
			if (sequence.isCurrent(request)) error(cause);
		} finally {
			if (sequence.isCurrent(request)) busy(false);
		}
	};
	return {
		run,
		schedule() {
			if (disposed) return;
			sequence.begin();
			clearTimeout(timer);
			timer = setTimeout(() => {
				void run();
			}, 300);
		},
		dispose() {
			disposed = true;
			clearTimeout(timer);
			sequence.dispose();
		},
	};
}

export function filterBoardTasks<
	T extends {
		title?: string | null;
		description?: string | null;
		project_category_id?: number | string;
		assignees?: Array<{ id: number }>;
	},
>(
	tasks: T[],
	filter: {
		searchText?: string | null;
		selectedUser?: number;
		selectedCategory?: number;
	},
) {
	const search = (filter.searchText || '').toLowerCase();
	return tasks.filter(
		(task) =>
			(!search ||
				(task.title || '').toLowerCase().includes(search) ||
				(task.description || '').toLowerCase().includes(search)) &&
			(!filter.selectedUser ||
				task.assignees?.some(
					(user) => Number(user.id) === Number(filter.selectedUser),
				)) &&
			(!filter.selectedCategory ||
				Number(task.project_category_id) === Number(filter.selectedCategory)),
	);
}
