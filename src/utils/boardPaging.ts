export interface PagedTask {
	id: number;
	common_time?: number;
}

export interface ColumnRest {
	count: number;
	seconds: number;
	overtime: number;
}

export interface StatusTotals {
	total: number;
	total_seconds: number;
	total_overtime_seconds: number;
}

export function appendUnknownTasks<T extends PagedTask>(
	column: T[],
	fetched: T[],
): void {
	const known = new Set(column.map((task) => task.id));
	column.push(...fetched.filter((task) => !known.has(task.id)));
}

const sumSeconds = (tasks: PagedTask[]) =>
	tasks.reduce((acc, task) => acc + (task.common_time || 0), 0);

export function restAfterPage<T extends PagedTask>(
	totals: StatusTotals,
	loaded: T[],
	overtimeOf: (tasks: T[]) => number,
): ColumnRest {
	return {
		count: totals.total - loaded.length,
		seconds: totals.total_seconds - sumSeconds(loaded),
		overtime: totals.total_overtime_seconds - overtimeOf(loaded),
	};
}

export function consumeRest<T extends PagedTask>(
	rest: ColumnRest,
	page: T[],
	overtimeOf: (tasks: T[]) => number,
): void {
	rest.count -= page.length;
	rest.seconds -= sumSeconds(page);
	rest.overtime -= overtimeOf(page);
}

export function columnOrders(tasks: PagedTask[]) {
	return tasks.map(({ id }, index) => ({ id, order: index + 1 }));
}
