import {
	beginOwnMove,
	compareTaskKeys,
	completeOwnMove,
	createColumnPaging,
	landPage,
	loadAllPages,
	loadNextPage,
	mergeRealtimeTask,
	placementAt,
	reloadWindow,
	removeRealtimeTask,
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
			title: `t${r.id}`,
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
				!live ||
				live.status_id !== anchor.status_id ||
				live.order !== anchor.order
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
	row(id: number): Row {
		return { ...this.rows.find((r) => r.id === id)! };
	}
	remove(id: number) {
		this.rows = this.rows.filter((r) => r.id !== id);
	}
	renumber(ids: number[]) {
		ids.forEach((id, i) => {
			this.rows.find((r) => r.id === id)!.order = (i + 1) * 1024;
		});
	}
	moveAfter(id: number, statusId: number, afterId: number): Row {
		const col = this.column(statusId);
		const after = col.findIndex((r) => r.id === afterId);
		const next = col[after + 1];
		const row = this.rows.find((r) => r.id === id)!;
		row.status_id = statusId;
		row.order = next
			? Math.floor((col[after].order! + next.order!) / 2)
			: col[after].order! + 1024;
		return { ...row };
	}
	moveBottom(id: number): Row {
		const row = this.rows.find((r) => r.id === id)!;
		const max = Math.max(
			...this.column(row.status_id)
				.filter((r) => r.id !== id)
				.map((r) => r.order ?? 0),
		);
		row.order = max + 1024;
		return { ...row };
	}
	moveToStatus(id: number, statusId: number) {
		this.rows.find((r) => r.id === id)!.status_id = statusId;
	}
	ids(statusId: number, category = 0) {
		return this.column(statusId, category).map((r) => r.id);
	}
}

class Gate {
	private open = false;
	private queue: Array<() => void> = [];
	defer<T>(compute: () => T, atRelease = false): Promise<T> {
		const snapshot = atRelease ? undefined : compute();
		return new Promise((resolve) => {
			const run = () => resolve(atRelease ? compute() : snapshot!);
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
	atRelease: boolean;
	load(statusIds: number[]): void;
	requestNext(statusId: number): Promise<void>;
	setCategory(category: number): void;
	moveLocally(id: number, from: number, to: number, afterId: number): void;
	move(id: number, from: number, to: number, afterId: number): Promise<void>;
	event(id: number): void;
	remove(id: number): void;
	drain(statusId: number): Promise<void>;
	boards(): Record<number, number[]>;
}

const placeLocally = (
	columns: Col[],
	id: number,
	from: number,
	to: number,
	afterId: number,
) => {
	const source = columns.find((c) => c.status.id === from)!;
	const target = columns.find((c) => c.status.id === to)!;
	const task = source.tasks.find((t) => t.id === id)!;
	source.tasks.splice(source.tasks.indexOf(task), 1);
	target.tasks.splice(
		target.tasks.findIndex((t) => t.id === afterId) + 1,
		0,
		task,
	);
};

const boardsOf = (columns: Col[]) =>
	Object.fromEntries(
		columns.map((c) => [c.status.id, c.tasks.map((t) => t.id)]),
	);

class KeysetImpl implements Impl {
	columns: Col[] = [];
	pending = new Set<number>();
	category = 0;
	atRelease = false;
	constructor(
		private server: FakeServer,
		private gate: Gate,
	) {}
	fetchPage = (statusId: number, cursor: string | null, limit: number) => {
		const category = this.category;
		return this.gate.defer(
			() => this.server.keysetPage(statusId, cursor, limit, category),
			this.atRelease,
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
	moveLocally(id: number, from: number, to: number, afterId: number) {
		placeLocally(this.columns, id, from, to, afterId);
	}
	move(id: number, from: number, to: number, afterId: number) {
		this.moveLocally(id, from, to, afterId);
		beginOwnMove(this.columns, id, [from, to], this.pending);
		return completeOwnMove({
			getColumns: () => this.columns,
			taskId: id,
			sourceStatus: from,
			targetStatus: to,
			pending: this.pending,
			send: async () => ({ data: this.server.moveAfter(id, to, afterId) }),
			fetchPage: this.fetchPage,
			limit: PER,
		}).then(() => undefined);
	}
	event(id: number) {
		mergeRealtimeTask(this.columns, this.server.row(id), this.pending);
	}
	remove(id: number) {
		this.server.remove(id);
		removeRealtimeTask(this.columns, id);
	}
	drain(statusId: number) {
		return loadAllPages(() => this.columns, statusId, this.fetchPage, {
			pending: this.pending,
		}).then(() => undefined);
	}
	boards() {
		return boardsOf(this.columns);
	}
}

class NaiveImpl implements Impl {
	columns: Col[] = [];
	pages = new Map<number, number>();
	category = 0;
	atRelease = false;
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
		const rows = await this.gate.defer(
			() => this.server.offsetPage(statusId, page, category),
			this.atRelease,
		);
		this.pages.set(statusId, page);
		this.columns.find((c) => c.status.id === statusId)?.tasks.push(...rows);
	}
	setCategory(category: number) {
		this.category = category;
		this.load(this.columns.map((c) => Number(c.status.id)));
	}
	moveLocally(id: number, from: number, to: number, afterId: number) {
		placeLocally(this.columns, id, from, to, afterId);
	}
	async move(id: number, from: number, to: number, afterId: number) {
		this.moveLocally(id, from, to, afterId);
		upsertTaskInColumns(this.columns, this.server.moveAfter(id, to, afterId));
	}
	event(id: number) {
		upsertTaskInColumns(this.columns, this.server.row(id));
	}
	remove(id: number) {
		this.server.remove(id);
		removeTaskFromColumns(this.columns, id);
	}
	async drain(statusId: number) {
		for (let i = 0; i < 20; i++) {
			const column = this.columns.find((c) => c.status.id === statusId)!;
			const before = column.tasks.length;
			await this.requestNext(statusId);
			if (column.tasks.length - before < PER) return;
		}
	}
	boards() {
		return boardsOf(this.columns);
	}
}

const loaders: Array<[string, typeof KeysetImpl | typeof NaiveImpl, boolean]> =
	[
		['naive offset loader', NaiveImpl, false],
		['keyset loader', KeysetImpl, true],
	];

const six = (status: number, nulls = false, from = 1) =>
	Array.from({ length: 6 }, (_, i) => ({
		id: from + i,
		status_id: status,
		...(nulls ? { order: null } : {}),
	}));

const check = (
	passes: boolean,
	actual: Record<number, number[]>,
	expected: Record<number, number[]>,
	naive: Record<number, number[]>,
) => {
	expect(actual).toEqual(passes ? expected : naive);
};

describe.each([
	['ordered rows', false],
	['NULL orders', true],
])('column paging races with %s', (_label, nulls) => {
	describe.each(loaders)('%s', (_name, Loader, passes) => {
		it('race 1: a delete between pages does not skip a card', async () => {
			const server = new FakeServer(six(1, nulls));
			const gate = new Gate();
			const impl = new Loader(server, gate);
			impl.load([1]);
			const before = server.ids(1);
			impl.remove(before[1]);
			const next = impl.requestNext(1);
			gate.release();
			await next;
			await impl.drain(1);
			const expected = before.filter((id) => id !== before[1]);
			check(
				passes,
				impl.boards(),
				{ 1: expected },
				{ 1: expected.filter((id) => id !== expected[2]) },
			);
		});

		it('race 3: a page requested before a filter change does not bring filtered-out cards back', async () => {
			const server = new FakeServer(six(1, nulls));
			const gate = new Gate();
			const impl = new Loader(server, gate);
			impl.load([1]);
			const next = impl.requestNext(1);
			impl.setCategory(1);
			gate.release();
			await next;
			await impl.drain(1);
			const filtered = server.ids(1, 1);
			const stale = server.ids(1).slice(PER, 2 * PER);
			check(
				passes,
				impl.boards(),
				{ 1: filtered },
				{ 1: [...filtered, ...stale] },
			);
		});

		it('race 4: a renumber by another client does not skip or duplicate cards', async () => {
			const server = new FakeServer(six(1, nulls));
			const gate = new Gate();
			const impl = new Loader(server, gate);
			impl.load([1]);
			const loaded = server.ids(1).slice(0, PER);
			server.renumber([4, 5, 6, 1, 2, 3]);
			const next = impl.requestNext(1);
			gate.release();
			await next;
			await impl.drain(1);
			check(
				passes,
				impl.boards(),
				{ 1: [4, 5, 6, 1, 2, 3] },
				{ 1: [...loaded, 1, 2, 3] },
			);
		});
	});
});

describe.each(loaders)('%s', (_name, Loader, passes) => {
	it('race 2: a page read after an own move does not duplicate the moved card', async () => {
		const server = new FakeServer([
			...six(2),
			{ id: 7, status_id: 1, order: 100 },
			{ id: 8, status_id: 1, order: 200 },
		]);
		const gate = new Gate();
		const impl = new Loader(server, gate);
		impl.load([1, 2]);
		impl.atRelease = true;
		const next = impl.requestNext(2);
		await impl.move(7, 1, 2, 3);
		gate.release();
		await next;
		await impl.drain(2);
		check(
			passes,
			impl.boards(),
			{ 1: [8], 2: [1, 2, 3, 7, 4, 5, 6] },
			{ 1: [8], 2: [1, 2, 3, 7, 7, 4, 5, 6] },
		);
	});

	it('B5: a delete while a page is in flight does not bring the card back', async () => {
		const server = new FakeServer(six(1));
		const gate = new Gate();
		const impl = new Loader(server, gate);
		impl.load([1]);
		const next = impl.requestNext(1);
		impl.remove(5);
		gate.release();
		await next;
		await impl.drain(1);
		check(
			passes,
			impl.boards(),
			{ 1: [1, 2, 3, 4, 6] },
			{ 1: [1, 2, 3, 4, 5, 6] },
		);
	});

	it('B5: a foreign move while a page is in flight leaves the card in one column only', async () => {
		const server = new FakeServer([
			...six(1),
			{ id: 9, status_id: 2, order: 100 },
		]);
		const gate = new Gate();
		const impl = new Loader(server, gate);
		impl.load([1, 2]);
		const next = impl.requestNext(1);
		server.moveToStatus(5, 2);
		impl.event(5);
		gate.release();
		await next;
		await impl.drain(1);
		check(
			passes,
			impl.boards(),
			{ 1: [1, 2, 3, 4, 6], 2: [9, 5] },
			{ 1: [1, 2, 3, 4, 5, 6], 2: [5, 9] },
		);
	});
});

describe('own moves', () => {
	const setup = (
		rows: ConstructorParameters<typeof FakeServer>[0],
		statuses: number[],
	) => {
		const server = new FakeServer(rows);
		const gate = new Gate();
		gate.release();
		const impl = new KeysetImpl(server, gate);
		impl.load(statuses);
		return { server, impl };
	};

	it('B3: a failed cross-column move leaves the card in its source column only', async () => {
		const { server, impl } = setup(
			[
				{ id: 1, status_id: 1 },
				{ id: 2, status_id: 1 },
				{ id: 3, status_id: 2 },
				{ id: 4, status_id: 2 },
			],
			[1, 2],
		);
		impl.moveLocally(1, 1, 2, 3);
		beginOwnMove(impl.columns, 1, [1, 2], impl.pending);
		const result = await completeOwnMove({
			getColumns: () => impl.columns,
			taskId: 1,
			sourceStatus: 1,
			targetStatus: 2,
			pending: impl.pending,
			send: () => Promise.reject(new Error('409')),
			fetchPage: impl.fetchPage,
			limit: PER,
		});
		expect(result.ok).toBe(false);
		expect(impl.boards()).toEqual({ 1: server.ids(1), 2: server.ids(2) });
		expect(impl.boards()).toEqual({ 1: [1, 2], 2: [3, 4] });
		expect(impl.pending.size).toBe(0);
	});

	it('B4: a card moved to the bottom of a partial column is restored when its page landed while the move was pending', async () => {
		const rows = Array.from({ length: 5 }, (_, i) => ({
			id: i + 1,
			status_id: 1,
		}));
		const { server, impl } = setup(rows, [1]);
		const column = impl.columns[0];
		column.tasks.splice(0, 1);
		beginOwnMove(impl.columns, 1, [1], impl.pending);
		let ack!: () => void;
		const moving = completeOwnMove({
			getColumns: () => impl.columns,
			taskId: 1,
			sourceStatus: 1,
			targetStatus: 1,
			pending: impl.pending,
			send: () =>
				new Promise<{ data: Row }>((resolve) => {
					const data = server.moveBottom(1);
					ack = () => resolve({ data });
				}),
			fetchPage: impl.fetchPage,
			limit: PER,
		});
		await impl.requestNext(1);
		expect(column.tasks.map((t) => t.id)).toEqual([2, 3, 4, 5]);
		ack();
		await moving;
		expect(impl.boards()).toEqual({ 1: server.ids(1) });
		expect(impl.boards()).toEqual({ 1: [2, 3, 4, 5, 1] });
	});

	it('B4: a card moved to the bottom stays out of the window until its page arrives', async () => {
		const { server, impl } = setup(six(1), [1]);
		impl.columns[0].tasks.splice(0, 1);
		beginOwnMove(impl.columns, 1, [1], impl.pending);
		await completeOwnMove({
			getColumns: () => impl.columns,
			taskId: 1,
			sourceStatus: 1,
			targetStatus: 1,
			pending: impl.pending,
			send: async () => ({ data: server.moveBottom(1) }),
			fetchPage: impl.fetchPage,
			limit: PER,
		});
		expect(impl.boards()).toEqual({ 1: [2, 3] });
		await impl.drain(1);
		expect(impl.boards()).toEqual({ 1: [2, 3, 4, 5, 6, 1] });
	});
});

describe('server resets', () => {
	it('a page after a deleted anchor reloads the column from the top', async () => {
		const server = new FakeServer(six(1));
		const gate = new Gate();
		gate.release();
		const impl = new KeysetImpl(server, gate);
		impl.load([1]);
		impl.remove(3);
		await impl.requestNext(1);
		await impl.drain(1);
		expect(impl.boards()).toEqual({ 1: server.ids(1) });
		expect(impl.boards()).toEqual({ 1: [1, 2, 4, 5, 6] });
	});

	describe('reloadWindow', () => {
		const make = (count: number, start = 0): Row[] =>
			Array.from({ length: count }, (_, i) => ({
				id: start + i + 1,
				status_id: 1,
				order: start + i + 1,
			}));
		const column = (tasks: Row[]): Col => ({
			status: { id: 1 },
			tasks,
			paging: createColumnPaging({ next_cursor: 'old' }),
		});

		it('restarts from the top when a later page resets', async () => {
			const columns = [column(make(201))];
			const fresh = make(201, 1000);
			const calls: Array<string | null> = [];
			let resets = 1;
			const fetchPage = async (
				_status: number,
				cursor: string | null,
			): Promise<PageResponse<Row>> => {
				calls.push(cursor);
				if (cursor === null)
					return {
						data: fresh.slice(0, 200),
						meta: { next_cursor: 'c1', total: 201 },
					};
				if (resets-- > 0)
					return { data: [], meta: { next_cursor: null, reset: true } };
				return { data: fresh.slice(200), meta: { next_cursor: null } };
			};
			await reloadWindow(() => columns, 1, fetchPage);
			expect(columns[0].tasks.map((t) => t.id)).toEqual(fresh.map((t) => t.id));
			expect(columns[0].paging!.complete).toBe(true);
			expect(calls).toEqual([null, 'c1', null, 'c1']);
		});

		it('keeps the current cards and leaves the column incomplete after too many resets', async () => {
			const original = make(201);
			const columns = [column(original.slice())];
			const fetchPage = async (
				_status: number,
				cursor: string | null,
			): Promise<PageResponse<Row>> =>
				cursor === null
					? { data: make(200, 1000), meta: { next_cursor: 'c1' } }
					: { data: [], meta: { next_cursor: null, reset: true } };
			await reloadWindow(() => columns, 1, fetchPage);
			expect(columns[0].tasks.map((t) => t.id)).toEqual(
				original.map((t) => t.id),
			);
			expect(columns[0].paging!.complete).toBe(false);
			expect(columns[0].paging!.cursor).toBe('old');
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

	it('B1: a full payload without order is a NULL order and goes to the top', () => {
		const col = column([1, 2, 3], true);
		const columns = [col, other()];
		const patch = mergeRealtimeTask(columns, {
			id: 12,
			status_id: 1,
			title: 'fresh',
		});
		expect(patch).toBe('inserted');
		expect(ids(col)).toEqual([12, 1, 2, 3]);
		expect(col.tasks[0].order).toBeNull();
	});

	it('B1: a NULL-order card is inside a partial window, never beyond it', () => {
		const col = column([1, 2, 3]);
		const columns = [col, other()];
		mergeRealtimeTask(columns, { id: 12, status_id: 1, title: 'fresh' });
		expect(ids(col)).toEqual([12, 1, 2, 3]);
	});

	it('B1: a partial payload without order reloads the target column', () => {
		const col = column([1, 2, 3]);
		const columns = [col, other()];
		expect(mergeRealtimeTask(columns, { id: 9, status_id: 1 })).toBe('reload');
		expect(ids(col)).toEqual([1, 2, 3]);
		expect(columns[1].tasks).toEqual([]);
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
