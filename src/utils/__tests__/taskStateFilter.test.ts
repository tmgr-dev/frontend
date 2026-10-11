import {
	filterTasksByState,
	paginateLocally,
	type TaskStateFilter,
} from '@/utils/taskStateFilter';

const statuses = [
	{ id: 1, type: 'default' },
	{ id: 2, type: 'completed' },
	{ id: 3, type: 'archived' },
];
const tasks = [
	{ id: 10, status_id: 1 },
	{ id: 11, status_id: 2 },
	{ id: 12, status_id: null },
	{ id: 13, status_id: 2 },
];

const filterCases: { filter: TaskStateFilter; ids: number[] }[] = [
	{ filter: 'all', ids: [10, 11, 12, 13] },
	{ filter: 'open', ids: [10, 12] },
	{ filter: 'done', ids: [11, 13] },
];

describe('filterTasksByState', () => {
	it.each(filterCases)('$filter', ({ filter, ids }) => {
		expect(
			filterTasksByState(tasks, filter, statuses).map((t) => t.id),
		).toEqual(ids);
	});
});

describe('paginateLocally', () => {
	const items = Array.from({ length: 25 }, (_, i) => i + 1);

	it('slices the requested page and builds the meta', () => {
		const { data, meta } = paginateLocally(items, 2, 10);
		expect(data).toEqual(items.slice(10, 20));
		expect(meta).toMatchObject({
			current_page: 2,
			per_page: 10,
			total: 25,
			last_page: 3,
			from: 11,
			to: 20,
		});
	});

	it('clamps a page past the end to the last page', () => {
		const { data, meta } = paginateLocally(items, 9, 10);
		expect(meta.current_page).toBe(3);
		expect(data).toEqual(items.slice(20));
	});

	it('handles an empty list', () => {
		const { data, meta } = paginateLocally([], 1, 10);
		expect(data).toEqual([]);
		expect(meta).toMatchObject({
			current_page: 1,
			total: 0,
			last_page: 1,
			from: 0,
			to: 0,
		});
	});
});
