import {
	normalizePriority,
	PRIORITIES,
	priorityOption,
	showPriorityBadge,
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
