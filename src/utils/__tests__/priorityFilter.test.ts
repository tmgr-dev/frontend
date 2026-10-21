import {
	matchesPriorityFilter,
	parsePriorityFilter,
	priorityRank,
	serializePriorityFilter,
	sortByPriority,
} from '../priorityFilter';

describe('priority filter', () => {
	it('ranks urgent, high, medium, low and treats missing or unknown as medium', () => {
		expect(priorityRank({ priority: 'urgent' })).toBe(0);
		expect(priorityRank({ priority: 'high' })).toBe(1);
		expect(priorityRank({ priority: 'medium' })).toBe(2);
		expect(priorityRank({ priority: 'low' })).toBe(3);
		expect(priorityRank({ priority: 'HIGH' })).toBe(1);
		expect(priorityRank({})).toBe(2);
		expect(priorityRank({ priority: null })).toBe(2);
		expect(priorityRank({ priority: 'critical' })).toBe(2);
	});

	it('parses a csv in rank order and ignores garbage', () => {
		expect(parsePriorityFilter('low,urgent')).toEqual(['urgent', 'low']);
		expect(parsePriorityFilter('foo')).toEqual([]);
		expect(parsePriorityFilter('high,foo,,high')).toEqual(['high']);
		expect(parsePriorityFilter(['medium', 'low,bar'])).toEqual([
			'medium',
			'low',
		]);
		expect(parsePriorityFilter(undefined)).toEqual([]);
		expect(parsePriorityFilter('')).toEqual([]);
		expect(serializePriorityFilter(['low', 'high', 'nope'])).toBe('high,low');
		expect(serializePriorityFilter([])).toBe('');
	});

	it('matches selected levels, missing priority as medium, empty as no filter', () => {
		expect(
			matchesPriorityFilter({ priority: 'high' }, ['urgent', 'high']),
		).toBe(true);
		expect(matchesPriorityFilter({ priority: 'low' }, ['urgent', 'high'])).toBe(
			false,
		);
		expect(matchesPriorityFilter({}, ['medium'])).toBe(true);
		expect(matchesPriorityFilter({ priority: 'weird' }, ['high'])).toBe(false);
		expect(matchesPriorityFilter({ priority: 'low' }, [])).toBe(true);
	});

	it('sorts stably and keeps the input untouched', () => {
		const tasks = [
			{ id: 1, priority: 'low' },
			{ id: 2 },
			{ id: 3, priority: 'urgent' },
			{ id: 4, priority: 'medium' },
			{ id: 5, priority: 'high' },
			{ id: 6, priority: 'urgent' },
			{ id: 7, priority: 'unknown' },
		];
		expect(sortByPriority(tasks).map((task) => task.id)).toEqual([
			3, 6, 5, 2, 4, 7, 1,
		]);
		expect(tasks.map((task) => task.id)).toEqual([1, 2, 3, 4, 5, 6, 7]);
	});
});
