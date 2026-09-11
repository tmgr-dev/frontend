/**
 * In-place patches for the board columns and the task list (TM-202 / TM-159): a created,
 * saved, archived or deleted task touches only its own card instead of reloading everything.
 * The functions mutate the arrays they are given (Vue reactive arrays included) and report
 * what they did so callers can refresh derived numbers only when something changed.
 */

export interface PatchableTask {
	id: number;
	status_id?: number | string | null;
	[key: string]: unknown;
}

export interface PatchableColumn {
	status?: { id?: number | string | null } | null;
	tasks: PatchableTask[];
}

export type ColumnPatch = 'inserted' | 'updated' | 'moved' | 'removed' | 'ignored';

const sameId = (a: unknown, b: unknown): boolean => a != null && b != null && Number(a) === Number(b);

/**
 * Put `task` where its status says: replace it in its column, move it to the top of another
 * column when the status changed, or insert it at the top when it is new. A task whose status
 * has no column on this board is dropped from the board ('removed') or ignored when unknown.
 */
export function upsertTaskInColumns(columns: PatchableColumn[], task: PatchableTask): ColumnPatch {
	const target = columns.find((c) => sameId(c.status?.id, task.status_id));
	for (const column of columns) {
		const index = column.tasks.findIndex((t) => t.id === task.id);
		if (index === -1) continue;
		const merged = { ...column.tasks[index], ...task };
		if (column === target) {
			column.tasks.splice(index, 1, merged);
			return 'updated';
		}
		column.tasks.splice(index, 1);
		if (!target) return 'removed';
		target.tasks.unshift(merged);
		return 'moved';
	}
	if (!target) return 'ignored';
	target.tasks.unshift(task);
	return 'inserted';
}

export function removeTaskFromColumns(columns: PatchableColumn[], taskId: number): boolean {
	for (const column of columns) {
		const index = column.tasks.findIndex((t) => t.id === taskId);
		if (index !== -1) {
			column.tasks.splice(index, 1);
			return true;
		}
	}
	return false;
}

export type ListPatch = 'inserted' | 'updated' | 'removed' | 'ignored';

/**
 * Same idea for a flat, filtered, paginated list: a task that still matches the filters is
 * replaced in place, one that stopped matching is dropped, and a new matching task is put on
 * top of the first page only (other pages would shift the server's paging).
 */
export function upsertTaskInList(
	tasks: PatchableTask[],
	task: PatchableTask,
	options: { accepts: (task: PatchableTask) => boolean; firstPage: boolean },
): ListPatch {
	const index = tasks.findIndex((t) => t.id === task.id);
	const accepted = options.accepts(task);
	if (index !== -1) {
		if (accepted) {
			tasks.splice(index, 1, task);
			return 'updated';
		}
		tasks.splice(index, 1);
		return 'removed';
	}
	if (accepted && options.firstPage) {
		tasks.unshift(task);
		return 'inserted';
	}
	return 'ignored';
}

export function removeTaskFromList(tasks: PatchableTask[], taskId: number): boolean {
	const index = tasks.findIndex((t) => t.id === taskId);
	if (index === -1) return false;
	tasks.splice(index, 1);
	return true;
}
