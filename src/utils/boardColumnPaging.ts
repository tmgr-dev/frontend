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
			if (!paging || paging.complete) return;
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
			if (result === 'applied') return;
			if (result === 'reset') {
				await reloadWindow(getColumns, statusId, fetchPage, options);
				return;
			}
		}
	} finally {
		const paging = findColumn(getColumns(), statusId)?.paging;
		if (paging) paging.loading = false;
		initial.loading = false;
	}
}

/**
 * Reload a column's loaded window from the top. The window keeps its size so the
 * board does not jump; a newer reload or own move drops this one.
 */
export async function reloadWindow<T extends PagedTask>(
	getColumns: () => Array<PagedColumn<T>>,
	statusId: number | string,
	fetchPage: FetchPage<T>,
	options: { limit?: number; pending?: ReadonlySet<number> } = {},
): Promise<void> {
	const column = findColumn(getColumns(), statusId);
	if (!column?.paging) return;
	const paging = column.paging;
	const size = Math.max(column.tasks.length, options.limit ?? BOARD_PAGE_SIZE);
	const epoch = nextEpoch();
	paging.epoch = epoch;
	const incoming: T[] = [];
	let cursor: string | null = null;
	let meta: PageMeta | undefined;
	do {
		const response: PageResponse<T> = await fetchPage(
			Number(statusId),
			cursor,
			pageLimit(size - incoming.length),
		);
		const current = findColumn(getColumns(), statusId)?.paging;
		if (!current || current.epoch !== epoch) return;
		if (response.meta?.reset && cursor !== null) {
			cursor = null;
			incoming.length = 0;
			continue;
		}
		incoming.push(...response.data);
		cursor = response.meta?.next_cursor ?? null;
		meta ??= response.meta;
	} while (cursor !== null && incoming.length < size);
	const target = findColumn(getColumns(), statusId);
	if (!target?.paging || target.paging.epoch !== epoch) return;
	const pending = options.pending ?? new Set<number>();
	const seen = idsOutside(getColumns() as PagedColumn[], target);
	const next: T[] = [];
	for (const task of incoming) {
		if (seen.has(task.id) || pending.has(task.id)) continue;
		seen.add(task.id);
		next.push(task);
	}
	for (const task of target.tasks)
		if (pending.has(task.id) && !next.some((t) => t.id === task.id))
			next.push(task);
	target.tasks.splice(0, target.tasks.length, ...next);
	target.paging.cursor = cursor;
	target.paging.complete = cursor === null;
	applyTotals(target.paging, meta);
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

/**
 * A foreign `task.updated`: paged columns place the card by its key when the key is
 * inside the loaded window, otherwise the card leaves the window and arrives with its
 * page. A card with a pending own move keeps its local placement.
 */
export function mergeRealtimeTask(
	columns: PagedColumn[],
	task: PagedTask,
	pending: ReadonlySet<number> = new Set(),
): ColumnPatch {
	const target = findColumn(columns, task.status_id);
	const holder = columns.find((c) => c.tasks.some((t) => t.id === task.id));
	if (!columns.some((c) => c.paging)) return upsertTaskInColumns(columns, task);
	if (holder && pending.has(task.id)) {
		const local = holder.tasks.find((t) => t.id === task.id)!;
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
	const keyed = target.paging && task.order !== undefined;
	if (!target.paging || !keyed) {
		if (holder === target) {
			const index = target.tasks.findIndex((t) => t.id === task.id);
			target.tasks.splice(index, 1, { ...target.tasks[index], ...task });
			return 'updated';
		}
		if (holder) removeTaskFromColumns(columns, task.id);
		if (target.paging) return holder ? 'removed' : 'ignored';
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

export type MovePlacement =
	{ after_id: number } | { before_id: number } | { position: 'top' | 'bottom' };

/** Anchor for a card just dropped at `index` of the column's loaded list. */
export function placementAt(tasks: PagedTask[], index: number): MovePlacement {
	if (index > 0) return { after_id: tasks[index - 1].id };
	if (tasks.length > 1) return { before_id: tasks[1].id };
	return { position: 'top' };
}
