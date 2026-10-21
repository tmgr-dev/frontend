import {
	matchesPriorityFilter,
	normalizePriority,
	parsePriorityFilter,
	PRIORITIES,
	priorityOption,
	priorityRank,
	serializePriorityFilter,
	showPriorityBadge,
	sortByPriority,
} from '@/utils/priority';

describe('priority helper', () => {
	it('lists the four levels from Urgent to Low', () => {
		expect(PRIORITIES.map((p) => p.label)).toEqual([
			'Urgent',
			'High',
			'Medium',
			'Low',
		]);
	});

	it.each([null, undefined, '', 'critical', 3])(
		'treats %p as medium',
		(value) => {
			expect(normalizePriority(value)).toBe('medium');
			expect(priorityOption(value).label).toBe('Medium');
			expect(showPriorityBadge(value)).toBe(false);
		},
	);

	it.each(['urgent', 'high', 'low'])('shows a badge for %s', (value) => {
		expect(normalizePriority(value)).toBe(value);
		expect(showPriorityBadge(value)).toBe(true);
	});

	it('hides the badge for medium', () => {
		expect(showPriorityBadge('medium')).toBe(false);
	});

	it('has a dark variant for every color', () => {
		for (const p of PRIORITIES) expect(p.colorClass).toMatch(/ dark:text-/);
	});
});

describe('priority filter', () => {
	it('ranks urgent, high, medium, low and treats missing or unknown as medium', () => {
		expect(priorityRank({ priority: 'urgent' })).toBe(0);
		expect(priorityRank({ priority: 'high' })).toBe(1);
		expect(priorityRank({ priority: 'medium' })).toBe(2);
		expect(priorityRank({ priority: 'low' })).toBe(3);
		expect(priorityRank({})).toBe(2);
		expect(priorityRank({ priority: null })).toBe(2);
		expect(priorityRank({ priority: 'critical' })).toBe(2);
	});

	it('parses a csv in rank order and ignores garbage', () => {
		expect(parsePriorityFilter('low,urgent')).toEqual(['urgent', 'low']);
		expect(parsePriorityFilter('foo')).toEqual([]);
		expect(parsePriorityFilter(' High ')).toEqual(['high']);
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
