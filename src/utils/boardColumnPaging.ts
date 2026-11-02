import {
	removeTaskFromColumns,
	upsertTaskInColumns,
	type ColumnPatch,
} from './taskPatch';

export const BOARD_PAGE_SIZE = 50;
const MAX_PAGE_SIZE = 200;
const MAX_ATTEMPTS = 3;

export interface PagedTask {
	id: number;
	status_id?: number | string | null;
	order?: number | null;
	[key: string]: unknown;
}

export interface PageMeta {
	next_cursor?: string | null;
	per_page?: number;
	total?: number;
	total_seconds?: number;
	total_overtime_seconds?: number;
	reset?: boolean;
}

export interface PageResponse<T extends PagedTask = PagedTask> {
	data: T[];
	meta?: PageMeta;
}

export type FetchPage<T extends PagedTask = PagedTask> = (
	statusId: number,
	cursor: string | null,
	limit: number,
) => Promise<PageResponse<T>>;

export interface ColumnPaging {
	cursor: string | null;
	complete: boolean;
	epoch: number;
	loading: boolean;
	settled: number;
	reload?: number;
	total?: number;
	totalSeconds?: number;
	totalOvertimeSeconds?: number;
}

export interface PagedColumn<T extends PagedTask = PagedTask> {
	status: { id?: number | string | null };
	tasks: T[];
	paging?: ColumnPaging;
}

let epochCounter = 0;
export const nextEpoch = () => ++epochCounter;

const sameId = (a: unknown, b: unknown): boolean =>
	a != null && b != null && Number(a) === Number(b);

export function isKeysetResponse(response: PageResponse | undefined): boolean {
	return !!response?.meta && 'next_cursor' in response.meta;
}

/** Board order: `order` ascending with nulls first, then `id` descending. */
export function compareTaskKeys(
	a: { id: number; order?: number | null },
	b: { id: number; order?: number | null },
): number {
	const ao = a.order ?? null;
	const bo = b.order ?? null;
	if (ao !== bo) {
		if (ao === null) return -1;
		if (bo === null) return 1;
		return ao - bo;
	}
	return b.id - a.id;
}

export function createColumnPaging(meta: PageMeta | undefined): ColumnPaging {
	const cursor = meta?.next_cursor ?? null;
	return {
		cursor,
		complete: cursor === null,
		epoch: nextEpoch(),
		loading: false,
		settled: 0,
		total: meta?.total,
		totalSeconds: meta?.total_seconds,
		totalOvertimeSeconds: meta?.total_overtime_seconds,
	};
}

export function applyTotals(paging: ColumnPaging, meta: PageMeta | undefined) {
	if (meta?.total === undefined) return;
	paging.total = meta.total;
	paging.totalSeconds = meta.total_seconds;
	paging.totalOvertimeSeconds = meta.total_overtime_seconds;
}

export function bumpColumnEpochs(
	columns: PagedColumn[],
	statusIds: Array<number | string>,
) {
	for (const column of columns) {
		if (column.paging && statusIds.some((id) => sameId(id, column.status.id)))
			column.paging.epoch = nextEpoch();
	}
}

const bumpAll = (columns: PagedColumn[]) => {
	for (const column of columns)
		if (column.paging) column.paging.epoch = nextEpoch();
};

const findColumn = <T extends PagedTask>(
	columns: Array<PagedColumn<T>>,
	statusId: number | string | null | undefined,
) => columns.find((column) => sameId(column.status.id, statusId));

const idsOutside = (columns: PagedColumn[], except?: PagedColumn) => {
	const ids = new Set<number>();
	for (const column of columns)
		if (column !== except) for (const task of column.tasks) ids.add(task.id);
	return ids;
};

/**
 * First pages of several columns fetched independently: a card shown by an earlier
 * column is skipped in later ones.
 */
export function dedupeFirstPages<T extends PagedTask>(
	pages: Array<PageResponse<T>>,
): T[][] {
	const seen = new Set<number>();
	return pages.map((page) =>
		page.data.filter((task) => {
			if (seen.has(task.id)) return false;
			seen.add(task.id);
			return true;
		}),
	);
}

export type LandResult = 'applied' | 'dropped' | 'reset';

/**
 * A page lands only when the column still has the epoch it was requested under.
 * Cards already shown anywhere on the board, or with a pending own move, are skipped.
 */
export function landPage<T extends PagedTask>(
	columns: Array<PagedColumn<T>>,
	statusId: number | string,
	epoch: number,
	response: PageResponse<T>,
	pending: ReadonlySet<number> = new Set(),
): LandResult {
	const column = findColumn(columns, statusId);
	const paging = column?.paging;
	if (!column || !paging || paging.epoch !== epoch) return 'dropped';
	if (response.meta?.reset) return 'reset';
	const seen = idsOutside(columns as PagedColumn[], column);
	for (const task of column.tasks) seen.add(task.id);
	for (const task of response.data) {
		if (seen.has(task.id) || pending.has(task.id)) continue;
		seen.add(task.id);
		column.tasks.push(task);
	}
	paging.cursor = response.meta?.next_cursor ?? null;
	paging.complete = paging.cursor === null;
	applyTotals(paging, response.meta);
	return 'applied';
}

const pageLimit = (size: number) => Math.max(1, Math.min(size, MAX_PAGE_SIZE));

export async function loadNextPage<T extends PagedTask>(
	getColumns: () => Array<PagedColumn<T>>,
	statusId: number | string,
	fetchPage: FetchPage<T>,
	options: { limit?: number; pending?: ReadonlySet<number> } = {},
): Promise<void> {
	const limit = pageLimit(options.limit ?? BOARD_PAGE_SIZE);
	const initial = findColumn(getColumns(), statusId)?.paging;
	if (!initial || initial.complete || initial.loading) return;
	initial.loading = true;
	try {
		for (let attempt = 0; attempt < MAX_ATTEMPTS; attempt++) {
			const paging = findColumn(getColumns(), statusId)?.paging;
			if (!paging || paging.complete) break;
			const epoch = paging.epoch;
			const cursor = paging.cursor;
			const response = await fetchPage(Number(statusId), cursor, limit);
			const result = landPage(
				getColumns(),
				statusId,
				epoch,
				response,
				options.pending,
			);
			if (result === 'applied') break;
			if (result === 'reset') {
				await reloadWindow(getColumns, statusId, fetchPage, options);
				break;
			}
		}
		const settled = findColumn(getColumns(), statusId)?.paging;
		if (settled) settled.settled++;
	} finally {
		const paging = findColumn(getColumns(), statusId)?.paging;
		if (paging) paging.loading = false;
		initial.loading = false;
	}
}

/**
 * Reload a column's loaded window from the top. The window keeps its size so the
 * board does not jump. A newer reload or a replaced column drops this one; an epoch
 * bump from a move or a realtime event, or a reset from the server, restarts it.
 * After too many restarts the column is left untouched.
 */
export async function reloadWindow<T extends PagedTask>(
	getColumns: () => Array<PagedColumn<T>>,
	statusId: number | string,
	fetchPage: FetchPage<T>,
	options: { limit?: number } = {},
): Promise<void> {
	const column = findColumn(getColumns(), statusId);
	if (!column?.paging) return;
	const paging = column.paging;
	const size = Math.max(column.tasks.length, options.limit ?? BOARD_PAGE_SIZE);
	const id = nextEpoch();
	paging.reload = id;
	for (let attempt = 0; attempt < MAX_ATTEMPTS; attempt++) {
		const epoch = nextEpoch();
		paging.epoch = epoch;
		const incoming: T[] = [];
		let cursor: string | null = null;
		let meta: PageMeta | undefined;
		let restart = false;
		for (;;) {
			const response: PageResponse<T> = await fetchPage(
				Number(statusId),
				cursor,
				pageLimit(size - incoming.length),
			);
			const current = findColumn(getColumns(), statusId)?.paging;
			if (current !== paging || paging.reload !== id) return;
			if (paging.epoch !== epoch || response.meta?.reset) {
				restart = true;
				break;
			}
			incoming.push(...response.data);
			cursor = response.meta?.next_cursor ?? null;
			meta ??= response.meta;
			if (cursor === null || incoming.length >= size) break;
		}
		if (restart) continue;
		const target = findColumn(getColumns(), statusId)!;
		const seen = idsOutside(getColumns() as PagedColumn[], target);
		const next: T[] = [];
		for (const task of incoming) {
			if (seen.has(task.id)) continue;
			seen.add(task.id);
			next.push(task);
		}
		target.tasks.splice(0, target.tasks.length, ...next);
		paging.cursor = cursor;
		paging.complete = cursor === null;
		paging.settled++;
		applyTotals(paging, meta);
		return;
	}
}

export async function loadAllPages<T extends PagedTask>(
	getColumns: () => Array<PagedColumn<T>>,
	statusId: number | string,
	fetchPage: FetchPage<T>,
	options: { pending?: ReadonlySet<number> } = {},
): Promise<boolean> {
	for (let guard = 0; guard < 1000; guard++) {
		const paging = findColumn(getColumns(), statusId)?.paging;
		if (!paging || paging.complete) return true;
		if (paging.loading) {
			await new Promise((resolve) => setTimeout(resolve, 50));
			continue;
		}
		await loadNextPage(getColumns, statusId, fetchPage, {
			limit: MAX_PAGE_SIZE,
			pending: options.pending,
		});
	}
	return false;
}

export function isBeyondBoundary(
	column: PagedColumn,
	task: { id: number; order?: number | null },
): boolean {
	const paging = column.paging;
	if (!paging || paging.complete) return false;
	const last = column.tasks[column.tasks.length - 1];
	if (!last) return true;
	return compareTaskKeys(task, last) > 0;
}

function insertByKey(column: PagedColumn, task: PagedTask) {
	const index = column.tasks.findIndex((t) => compareTaskKeys(t, task) > 0);
	if (index === -1) column.tasks.push(task);
	else column.tasks.splice(index, 0, task);
}

const isFullTask = (task: PagedTask) => 'title' in task && 'status_id' in task;

/**
 * A foreign `task.updated`: paged columns place the card by its key when the key is
 * inside the loaded window, otherwise the card leaves the window and arrives with its
 * page. A card with a pending own move keeps its local placement. A full payload
 * without `order` means a NULL order; a partial one has an unknown key ('reload') and
 * its target column is reloaded. Every merge bumps the epochs so a page read before
 * the event cannot land over it.
 */
export function mergeRealtimeTask(
	columns: PagedColumn[],
	incoming: PagedTask,
	pending: ReadonlySet<number> = new Set(),
): ColumnPatch | 'reload' {
	if (!columns.some((c) => c.paging))
		return upsertTaskInColumns(columns, incoming);
	bumpAll(columns);
	const task =
		incoming.order === undefined && isFullTask(incoming)
			? { ...incoming, order: null }
			: incoming;
	const target = findColumn(columns, task.status_id);
	const holder = columns.find((c) => c.tasks.some((t) => t.id === task.id));
	if (pending.has(task.id)) {
		const local = holder?.tasks.find((t) => t.id === task.id);
		if (!local) return 'ignored';
		Object.assign(local, task, {
			status_id: local.status_id,
			order: local.order,
		});
		return 'updated';
	}
	if (!target) {
		if (!holder) return 'ignored';
		removeTaskFromColumns(columns, task.id);
		return 'removed';
	}
	if (!target.paging || task.order === undefined) {
		if (holder === target) {
			const index = target.tasks.findIndex((t) => t.id === task.id);
			target.tasks.splice(index, 1, { ...target.tasks[index], ...task });
			return 'updated';
		}
		if (holder) removeTaskFromColumns(columns, task.id);
		if (target.paging) return 'reload';
		target.tasks.unshift(task);
		return holder ? 'moved' : 'inserted';
	}
	const previous = holder?.tasks.find((t) => t.id === task.id);
	if (holder) removeTaskFromColumns(columns, task.id);
	if (isBeyondBoundary(target, task)) return holder ? 'removed' : 'ignored';
	insertByKey(target, previous ? { ...previous, ...task } : task);
	if (!holder) return 'inserted';
	return holder === target ? 'updated' : 'moved';
}

export function removeRealtimeTask(
	columns: PagedColumn[],
	taskId: number,
): boolean {
	bumpAll(columns);
	return removeTaskFromColumns(columns, taskId);
}

export function beginOwnMove(
	columns: PagedColumn[],
	taskId: number,
	statusIds: Array<number | string>,
	pending: Set<number>,
) {
	bumpColumnEpochs(columns, statusIds);
	pending.add(taskId);
}

export function applyMoveResult<T extends PagedTask>(
	columns: Array<PagedColumn<T>>,
	data: T,
): ColumnPatch | 'reload' {
	for (const column of columns) {
		const local = column.tasks.find((t) => t.id === data.id);
		if (local) {
			Object.assign(local, data);
			return 'updated';
		}
	}
	return mergeRealtimeTask(columns as PagedColumn[], data);
}

/**
 * Settle an own move after the request ran: apply the answer, or on failure take the
 * card off the board and reload source and then target, one after the other, so the
 * card cannot be dropped from both by cross-column dedupe.
 */
export async function completeOwnMove<T extends PagedTask>(options: {
	getColumns: () => Array<PagedColumn<T>>;
	taskId: number;
	sourceStatus: number;
	targetStatus: number;
	pending: Set<number>;
	send: () => Promise<{ data: T; meta?: { renumbered?: boolean } }>;
	fetchPage: FetchPage<T>;
	limit?: number;
}): Promise<{ ok: true } | { ok: false; error: unknown }> {
	const { getColumns, taskId, sourceStatus, targetStatus } = options;
	let reload: number[] = [];
	let outcome: { ok: true } | { ok: false; error: unknown } = { ok: true };
	try {
		const { data, meta } = await options.send();
		options.pending.delete(taskId);
		bumpColumnEpochs(getColumns(), [sourceStatus, targetStatus]);
		const patch = applyMoveResult(getColumns(), data);
		if (meta?.renumbered || patch === 'reload') reload = [targetStatus];
	} catch (error) {
		options.pending.delete(taskId);
		removeTaskFromColumns(getColumns(), taskId);
		bumpColumnEpochs(getColumns(), [sourceStatus, targetStatus]);
		reload = [...new Set([sourceStatus, targetStatus])];
		outcome = { ok: false, error };
	}
	for (const statusId of reload)
		await reloadWindow(getColumns, statusId, options.fetchPage, {
			limit: options.limit,
		});
	return outcome;
}

export type MovePlacement =
	{ after_id: number } | { before_id: number } | { position: 'top' | 'bottom' };

/** Anchor for a card just dropped at `index` of the column's loaded list. */
export function placementAt(tasks: PagedTask[], index: number): MovePlacement {
	if (index > 0) return { after_id: tasks[index - 1].id };
	if (tasks.length > 1) return { before_id: tasks[1].id };
	return { position: 'top' };
}
