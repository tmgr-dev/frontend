import {
	bumpColumnEpochs,
	compareTaskKeys,
	createColumnPaging,
	landPage,
	loadAllPages,
	loadNextPage,
	mergeRealtimeTask,
	placementAt,
	type PagedColumn,
	type PagedTask,
	type PageResponse,
} from '../boardColumnPaging';
import { removeTaskFromColumns, upsertTaskInColumns } from '../taskPatch';

interface Row extends PagedTask {
	id: number;
	status_id: number;
	order: number | null;
	category?: number;
}

const PER = 3;

class FakeServer {
	rows: Row[];
	constructor(rows: Array<Partial<Row> & { id: number; status_id: number }>) {
		this.rows = rows.map((r, i) => ({
			order: (i + 1) * 1024,
			category: r.id % 2 ? 1 : 2,
			...r,
		}));
	}
	column(statusId: number, category = 0) {
		return this.rows
			.filter(
				(r) =>
					r.status_id === statusId && (!category || r.category === category),
			)
			.sort(compareTaskKeys);
	}
	keysetPage(
		statusId: number,
		cursor: string | null,
		limit: number,
		category = 0,
	): PageResponse<Row> {
		let rows = this.column(statusId, category);
		if (cursor) {
			const anchor = JSON.parse(cursor) as {
				id: number;
				order: number | null;
				status_id: number;
			};
			const live = this.rows.find((r) => r.id === anchor.id);
			if (
				live &&
				(live.status_id !== anchor.status_id || live.order !== anchor.order)
			)
				return { data: [], meta: { next_cursor: null, reset: true } };
			rows = rows.filter((r) => compareTaskKeys(r, anchor) > 0);
		}
		const page = rows.slice(0, limit);
		const last = page[page.length - 1];
		return {
			data: page.map((r) => ({ ...r })),
			meta: {
				next_cursor:
					rows.length > limit
						? JSON.stringify({
								id: last.id,
								order: last.order,
								status_id: statusId,
							})
						: null,
				...(cursor ? {} : { total: this.column(statusId, category).length }),
			},
		};
	}
	offsetPage(statusId: number, page: number, category = 0): Row[] {
		return this.column(statusId, category)
			.slice((page - 1) * PER, page * PER)
			.map((r) => ({ ...r }));
	}
	remove(id: number) {
		this.rows = this.rows.filter((r) => r.id !== id);
	}
	renumber(statusId: number, ids: number[]) {
		ids.forEach((id, i) => {
			this.rows.find((r) => r.id === id)!.order = (i + 1) * 1024;
		});
		void statusId;
	}
	moveAfter(id: number, statusId: number, afterId: number) {
		const col = this.column(statusId);
		const after = col.findIndex((r) => r.id === afterId);
		const next = col[after + 1];
		const row = this.rows.find((r) => r.id === id)!;
		row.status_id = statusId;
		row.order = next
			? Math.floor((col[after].order! + next.order!) / 2)
			: col[after].order! + 1024;
	}
	ids(statusId: number, category = 0) {
		return this.column(statusId, category).map((r) => r.id);
	}
}

class Gate {
	private open = false;
	private queue: Array<() => void> = [];
	defer<T>(compute: () => T): Promise<T> {
		return new Promise((resolve) => {
			const run = () => resolve(compute());
			if (this.open) run();
			else this.queue.push(run);
		});
	}
	release() {
		this.open = true;
		this.queue.splice(0).forEach((run) => run());
	}
}

type Col = PagedColumn<Row>;

interface Impl {
	columns: Col[];
	load(statusIds: number[]): void;
	requestNext(statusId: number): Promise<void>;
	setCategory(category: number): void;
	drag(id: number, from: number, to: number, afterId: number): void;
	ackMove(id: number): void;
	remove(id: number): void;
	drain(statusId: number): Promise<void>;
	boards(): Record<number, number[]>;
}

class KeysetImpl implements Impl {
	columns: Col[] = [];
	pending = new Set<number>();
	category = 0;
	constructor(
		private server: FakeServer,
		private gate: Gate,
	) {}
	private fetchPage = (
		statusId: number,
		cursor: string | null,
		limit: number,
	) => {
		const category = this.category;
		return this.gate.defer(() =>
			this.server.keysetPage(statusId, cursor, limit, category),
		);
	};
	load(statusIds: number[]) {
		this.columns = statusIds.map((id) => {
			const page = this.server.keysetPage(id, null, PER, this.category);
			return {
				status: { id },
				tasks: page.data,
				paging: createColumnPaging(page.meta),
			};
		});
	}
	requestNext(statusId: number) {
		return loadNextPage(() => this.columns, statusId, this.fetchPage, {
			limit: PER,
			pending: this.pending,
		});
	}
	setCategory(category: number) {
		this.category = category;
		this.load(this.columns.map((c) => Number(c.status.id)));
	}
	drag(id: number, from: number, to: number, afterId: number) {
		const source = this.columns.find((c) => c.status.id === from)!;
		const target = this.columns.find((c) => c.status.id === to)!;
		const task = source.tasks.find((t) => t.id === id)!;
		source.tasks.splice(source.tasks.indexOf(task), 1);
		target.tasks.splice(
			target.tasks.findIndex((t) => t.id === afterId) + 1,
			0,
			task,
		);
		bumpColumnEpochs(this.columns, [from, to]);
		this.pending.add(id);
		this.server.moveAfter(id, to, afterId);
	}
	ackMove(id: number) {
		this.pending.delete(id);
		const row = this.server.rows.find((r) => r.id === id)!;
		mergeRealtimeTask(this.columns, { ...row }, this.pending);
	}
	remove(id: number) {
		this.server.remove(id);
		removeTaskFromColumns(this.columns, id);
	}
	drain(statusId: number) {
		return loadAllPages(() => this.columns, statusId, this.fetchPage, {
			pending: this.pending,
		}).then(() => undefined);
	}
	boards() {
		return Object.fromEntries(
			this.columns.map((c) => [c.status.id, c.tasks.map((t) => t.id)]),
		);
	}
}

class NaiveImpl implements Impl {
	columns: Col[] = [];
	pages = new Map<number, number>();
	category = 0;
	constructor(
		private server: FakeServer,
		private gate: Gate,
	) {}
	load(statusIds: number[]) {
		this.columns = statusIds.map((id) => {
			this.pages.set(id, 1);
			return {
				status: { id },
				tasks: this.server.offsetPage(id, 1, this.category),
			};
		});
	}
	async requestNext(statusId: number) {
		const page = (this.pages.get(statusId) ?? 1) + 1;
		const category = this.category;
		const rows = await this.gate.defer(() =>
			this.server.offsetPage(statusId, page, category),
		);
		this.pages.set(statusId, page);
		this.columns.find((c) => c.status.id === statusId)?.tasks.push(...rows);
	}
	setCategory(category: number) {
		this.category = category;
		this.load(this.columns.map((c) => Number(c.status.id)));
	}
	drag(id: number, from: number, to: number, afterId: number) {
		const source = this.columns.find((c) => c.status.id === from)!;
		const target = this.columns.find((c) => c.status.id === to)!;
		const task = source.tasks.find((t) => t.id === id)!;
		source.tasks.splice(source.tasks.indexOf(task), 1);
		target.tasks.splice(
			target.tasks.findIndex((t) => t.id === afterId) + 1,
			0,
			task,
		);
		this.server.moveAfter(id, to, afterId);
	}
	ackMove(id: number) {
		const row = this.server.rows.find((r) => r.id === id)!;
		upsertTaskInColumns(this.columns, { ...row });
	}
	remove(id: number) {
		this.server.remove(id);
		removeTaskFromColumns(this.columns, id);
	}
	async drain(statusId: number) {
		for (let i = 0; i < 20; i++) {
			const before = this.columns.find((c) => c.status.id === statusId)!.tasks
				.length;
			await this.requestNext(statusId);
			const after = this.columns.find((c) => c.status.id === statusId)!.tasks
				.length;
			if (after - before < PER) return;
		}
	}
	boards() {
		return Object.fromEntries(
			this.columns.map((c) => [c.status.id, c.tasks.map((t) => t.id)]),
		);
	}
}

const loaders: Array<[string, typeof KeysetImpl | typeof NaiveImpl, boolean]> =
	[
		['naive offset loader', NaiveImpl, false],
		['keyset loader', KeysetImpl, true],
	];

const six = (status: number, from = 1) =>
	Array.from({ length: 6 }, (_, i) => ({ id: from + i, status_id: status }));

const expectBoard = (
	passes: boolean,
	actual: Record<number, number[]>,
	expected: Record<number, number[]>,
) => {
	if (passes) expect(actual).toEqual(expected);
	else expect(actual).not.toEqual(expected);
};

describe('column paging races', () => {
	describe.each(loaders)('%s', (_name, Loader, passes) => {
		it('race 1: a delete between pages does not skip a card', async () => {
			const server = new FakeServer(six(1));
			const gate = new Gate();
			const impl = new Loader(server, gate);
			impl.load([1]);
			const next = impl.requestNext(1);
			impl.remove(2);
			gate.release();
			await next;
			await impl.drain(1);
			expectBoard(passes, impl.boards(), { 1: server.ids(1) });
		});

		it('race 2: a page requested before an own move does not duplicate the moved card', async () => {
			const server = new FakeServer([
				...six(2),
				{ id: 7, status_id: 1, order: 100 },
				{ id: 8, status_id: 1, order: 200 },
			]);
			const gate = new Gate();
			const impl = new Loader(server, gate);
			impl.load([1, 2]);
			const next = impl.requestNext(2);
			impl.drag(7, 1, 2, 1);
			gate.release();
			await next;
			impl.ackMove(7);
			await impl.drain(2);
			expectBoard(passes, impl.boards(), {
				1: server.ids(1),
				2: server.ids(2),
			});
		});

		it('race 3: a page requested before a filter change does not bring filtered-out cards back', async () => {
			const server = new FakeServer(six(1));
			const gate = new Gate();
			const impl = new Loader(server, gate);
			impl.load([1]);
			const next = impl.requestNext(1);
			impl.setCategory(1);
			gate.release();
			await next;
			await impl.drain(1);
			expectBoard(passes, impl.boards(), { 1: server.ids(1, 1) });
		});

		it('race 4: a renumber by another client does not skip or duplicate cards', async () => {
			const server = new FakeServer(six(1));
			const gate = new Gate();
			const impl = new Loader(server, gate);
			impl.load([1]);
			const next = impl.requestNext(1);
			server.renumber(1, [4, 5, 6, 1, 2, 3]);
			gate.release();
			await next;
			await impl.drain(1);
			expectBoard(passes, impl.boards(), { 1: [4, 5, 6, 1, 2, 3] });
		});
	});
});

describe('realtime merge in paged columns', () => {
	const column = (ids: number[], complete = false): Col => ({
		status: { id: 1 },
		tasks: ids.map((id) => ({
			id,
			status_id: 1,
			order: id * 1000,
			category: 1,
		})),
		paging: {
			...createColumnPaging({ next_cursor: complete ? null : 'c' }),
		},
	});
	const other = (): Col => ({
		status: { id: 2 },
		tasks: [{ id: 9, status_id: 2, order: 1000, category: 1 }],
		paging: createColumnPaging({ next_cursor: null }),
	});
	const ids = (c: Col) => c.tasks.map((t) => t.id);

	it('drops a foreign update beyond the loaded boundary and it arrives with its page', () => {
		const col = column([1, 2, 3]);
		const columns = [col, other()];
		const patch = mergeRealtimeTask(columns, {
			id: 9,
			status_id: 1,
			order: 5000,
			title: 'new',
		});
		expect(patch).toBe('removed');
		expect(ids(col)).toEqual([1, 2, 3]);
		expect(columns[1].tasks).toEqual([]);
		const epoch = col.paging!.epoch;
		landPage(columns, 1, epoch, {
			data: [
				{ id: 4, status_id: 1, order: 4000 },
				{ id: 9, status_id: 1, order: 5000, title: 'new' },
			],
			meta: { next_cursor: null },
		});
		expect(ids(col)).toEqual([1, 2, 3, 4, 9]);
		expect(col.tasks[4].title).toBe('new');
	});

	it('places an update before the boundary by its key', () => {
		const col = column([1, 2, 3]);
		const columns = [col, other()];
		expect(
			mergeRealtimeTask(columns, { id: 9, status_id: 1, order: 1500 }),
		).toBe('moved');
		expect(ids(col)).toEqual([1, 9, 2, 3]);
		expect(
			mergeRealtimeTask(columns, { id: 3, status_id: 1, order: 500 }),
		).toBe('updated');
		expect(ids(col)).toEqual([3, 1, 9, 2]);
	});

	it('places by key anywhere in a complete column', () => {
		const col = column([1, 2, 3], true);
		const columns = [col, other()];
		mergeRealtimeTask(columns, { id: 9, status_id: 1, order: 9000 });
		expect(ids(col)).toEqual([1, 2, 3, 9]);
	});

	it('a card with a pending own move ignores foreign placement', () => {
		const col = column([1, 2, 3]);
		const columns = [col, other()];
		const patch = mergeRealtimeTask(
			columns,
			{ id: 2, status_id: 2, order: 1, title: 'x' },
			new Set([2]),
		);
		expect(patch).toBe('updated');
		expect(ids(col)).toEqual([1, 2, 3]);
		expect(col.tasks[1]).toMatchObject({
			status_id: 1,
			order: 2000,
			title: 'x',
		});
	});

	it('legacy columns without paging keep the old upsert', () => {
		const columns: PagedColumn[] = [
			{ status: { id: 1 }, tasks: [{ id: 1, status_id: 1 }] },
			{ status: { id: 2 }, tasks: [] },
		];
		expect(mergeRealtimeTask(columns, { id: 1, status_id: 2 })).toBe('moved');
		expect(columns[1].tasks.map((t) => t.id)).toEqual([1]);
	});
});

describe('keys and placement', () => {
	it('orders nulls first, then by id descending', () => {
		const sorted = [
			{ id: 1, order: 5 },
			{ id: 2, order: null },
			{ id: 3, order: null },
			{ id: 4, order: 5 },
		].sort(compareTaskKeys);
		expect(sorted.map((t) => t.id)).toEqual([3, 2, 4, 1]);
	});

	it('anchors a drop to its neighbour', () => {
		const tasks = [{ id: 1 }, { id: 2 }, { id: 3 }];
		expect(placementAt(tasks, 2)).toEqual({ after_id: 2 });
		expect(placementAt(tasks, 0)).toEqual({ before_id: 2 });
		expect(placementAt([{ id: 1 }], 0)).toEqual({ position: 'top' });
	});
});
