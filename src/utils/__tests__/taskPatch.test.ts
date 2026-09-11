import {
	removeTaskFromColumns,
	removeTaskFromList,
	upsertTaskInColumns,
	upsertTaskInList,
	type PatchableColumn,
} from '../taskPatch';

const columns = (): PatchableColumn[] => [
	{ status: { id: 1 }, tasks: [{ id: 10, status_id: 1, title: 'a' }, { id: 11, status_id: 1, title: 'b' }] },
	{ status: { id: 2 }, tasks: [{ id: 20, status_id: '2', title: 'c' }] },
];

describe('upsertTaskInColumns', () => {
	it('inserts a new task at the top of its column', () => {
		const cols = columns();
		expect(upsertTaskInColumns(cols, { id: 12, status_id: 1, title: 'new' })).toBe('inserted');
		expect(cols[0].tasks.map((t) => t.id)).toEqual([12, 10, 11]);
	});

	it('replaces a task in place when the status is unchanged, keeping fields the patch lacks', () => {
		const cols = columns();
		expect(upsertTaskInColumns(cols, { id: 11, status_id: '1', common_time: 5 })).toBe('updated');
		expect(cols[0].tasks[1]).toEqual({ id: 11, status_id: '1', title: 'b', common_time: 5 });
	});

	it('moves a task to the top of another column when the status changed', () => {
		const cols = columns();
		expect(upsertTaskInColumns(cols, { id: 10, status_id: 2 })).toBe('moved');
		expect(cols[0].tasks.map((t) => t.id)).toEqual([11]);
		expect(cols[1].tasks.map((t) => t.id)).toEqual([10, 20]);
		expect(cols[1].tasks[0].title).toBe('a');
	});

	it('drops a task whose new status has no column, and ignores unknown tasks for unknown columns', () => {
		const cols = columns();
		expect(upsertTaskInColumns(cols, { id: 20, status_id: 99 })).toBe('removed');
		expect(cols[1].tasks).toEqual([]);
		expect(upsertTaskInColumns(cols, { id: 30, status_id: 99 })).toBe('ignored');
	});

	it('removes by id and reports whether anything was there', () => {
		const cols = columns();
		expect(removeTaskFromColumns(cols, 20)).toBe(true);
		expect(removeTaskFromColumns(cols, 20)).toBe(false);
		expect(cols[1].tasks).toEqual([]);
	});
});

describe('upsertTaskInList', () => {
	const accepts = (t: { status_id?: unknown }) => Number(t.status_id) === 1;

	it('replaces a matching task, drops one that stopped matching', () => {
		const tasks = [{ id: 1, status_id: 1 }, { id: 2, status_id: 1 }];
		expect(upsertTaskInList(tasks, { id: 2, status_id: 1, title: 'x' }, { accepts, firstPage: true })).toBe('updated');
		expect(tasks[1]).toEqual({ id: 2, status_id: 1, title: 'x' });
		expect(upsertTaskInList(tasks, { id: 1, status_id: 3 }, { accepts, firstPage: true })).toBe('removed');
		expect(tasks.map((t) => t.id)).toEqual([2]);
	});

	it('inserts a new matching task on top of the first page only', () => {
		const tasks = [{ id: 1, status_id: 1 }];
		expect(upsertTaskInList(tasks, { id: 5, status_id: 1 }, { accepts, firstPage: false })).toBe('ignored');
		expect(upsertTaskInList(tasks, { id: 5, status_id: 1 }, { accepts, firstPage: true })).toBe('inserted');
		expect(tasks.map((t) => t.id)).toEqual([5, 1]);
		expect(upsertTaskInList(tasks, { id: 6, status_id: 2 }, { accepts, firstPage: true })).toBe('ignored');
	});

	it('removes by id', () => {
		const tasks = [{ id: 1 }, { id: 2 }];
		expect(removeTaskFromList(tasks, 1)).toBe(true);
		expect(removeTaskFromList(tasks, 1)).toBe(false);
		expect(tasks).toEqual([{ id: 2 }]);
	});
});
