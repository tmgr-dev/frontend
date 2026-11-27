import {
	canReorderCards,
	isSubsetFiltered,
	unpagedDropSave,
} from '../boardDropPlan';

describe('isSubsetFiltered', () => {
	it('is off without filters', () => {
		expect(isSubsetFiltered({})).toBe(false);
		expect(isSubsetFiltered(null)).toBe(false);
		expect(isSubsetFiltered({ selectedPriorities: [] })).toBe(false);
	});

	it('is on for every filter that drops tasks from a column', () => {
		expect(isSubsetFiltered({ selectedUser: 3 })).toBe(true);
		expect(isSubsetFiltered({ selectedCategory: 7 })).toBe(true);
		expect(isSubsetFiltered({ selectedPersona: 'p-1' })).toBe(true);
		expect(isSubsetFiltered({ searchText: 'login' })).toBe(true);
		expect(isSubsetFiltered({ selectedPriorities: ['high'] })).toBe(true);
	});
});

describe('unpagedDropSave', () => {
	it('changes only the status while sorted by priority', () => {
		for (const subset of [false, true])
			for (const local of [false, true])
				expect(unpagedDropSave({ sorted: true, subset, local })).toBe('status');
	});

	it('renumbers the column when it holds every task', () => {
		expect(
			unpagedDropSave({ sorted: false, subset: false, local: false }),
		).toBe('orders');
		expect(unpagedDropSave({ sorted: false, subset: false, local: true })).toBe(
			'orders',
		);
	});

	it('places the card next to its visible neighbour on a filtered cloud board', () => {
		expect(unpagedDropSave({ sorted: false, subset: true, local: false })).toBe(
			'move',
		);
	});

	it('keeps hidden orders on a filtered local board by changing only the status', () => {
		expect(unpagedDropSave({ sorted: false, subset: true, local: true })).toBe(
			'status',
		);
	});
});

describe('canReorderCards', () => {
	it('allows reordering only when the order can be saved', () => {
		expect(canReorderCards({ sorted: false, subset: false, local: true })).toBe(
			true,
		);
		expect(canReorderCards({ sorted: false, subset: true, local: false })).toBe(
			true,
		);
		expect(canReorderCards({ sorted: false, subset: true, local: true })).toBe(
			false,
		);
		expect(canReorderCards({ sorted: true, subset: false, local: false })).toBe(
			false,
		);
	});
});
