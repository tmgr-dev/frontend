import {
	filterableStatuses,
	filterTasksByStatusIds,
	matchesStatusIds,
} from '@/utils/taskStatusFilter';

const tasks = [
	{ id: 1, status_id: 276 },
	{ id: 2, status_id: '277' },
	{ id: 3, status_id: null },
	{ id: 4, status_id: 316 },
];

describe('filterTasksByStatusIds', () => {
	it('returns everything when no status is selected', () => {
		expect(filterTasksByStatusIds(tasks, [])).toEqual(tasks);
	});

	it('keeps tasks of any selected status', () => {
		expect(filterTasksByStatusIds(tasks, [276, 316]).map((t) => t.id)).toEqual([
			1, 4,
		]);
	});

	it('compares ids numerically and drops tasks without a status', () => {
		expect(filterTasksByStatusIds(tasks, [277]).map((t) => t.id)).toEqual([2]);
	});

	it('matches a single task', () => {
		expect(matchesStatusIds({ status_id: 5 }, [5])).toBe(true);
		expect(matchesStatusIds({ status_id: 5 }, [6])).toBe(false);
		expect(matchesStatusIds({ status_id: null }, [6])).toBe(false);
		expect(matchesStatusIds({ status_id: null }, [])).toBe(true);
	});
});

describe('filterableStatuses', () => {
	it('drops archived and hidden statuses', () => {
		const statuses = [
			{ id: 1, type: 'default' },
			{ id: 2, type: 'active' },
			{ id: 3, type: 'completed' },
			{ id: 4, type: 'archived' },
			{ id: 5, type: 'hidden' },
		];
		expect(filterableStatuses(statuses).map((s) => s.id)).toEqual([1, 2, 3]);
		expect(filterableStatuses(null)).toEqual([]);
	});
});
