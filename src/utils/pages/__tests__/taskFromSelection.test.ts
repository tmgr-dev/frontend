import {
	LAST_CATEGORY_KEY,
	pickDefaultCategory,
	readLastCategory,
	rememberCategory,
	taskFromSelectionError,
	taskKeyLabel,
} from '../taskFromSelection';

describe('taskKeyLabel', () => {
	it('prefers an explicit key, then the title prefix, then the id', () => {
		expect(taskKeyLabel({ id: 3, key: 'TM-9', title: 'x' })).toBe('TM-9');
		expect(taskKeyLabel({ id: 3, title: 'TM-213: do it' })).toBe('TM-213');
		expect(taskKeyLabel({ id: 3, title: 'do it' })).toBe('#3');
	});
});

describe('pickDefaultCategory', () => {
	const categories = [{ id: 5 }, { id: 7 }];
	it('uses the last used category when it still exists', () => {
		expect(pickDefaultCategory(categories, 7)).toBe(7);
	});
	it('falls back to the first, or null when empty', () => {
		expect(pickDefaultCategory(categories, 99)).toBe(5);
		expect(pickDefaultCategory(categories, null)).toBe(5);
		expect(pickDefaultCategory([], 5)).toBeNull();
	});
});

describe('last category storage', () => {
	const store = new Map<string, string>();
	beforeEach(() => {
		store.clear();
		(globalThis as any).localStorage = {
			getItem: (key: string) => store.get(key) ?? null,
			setItem: (key: string, value: string) => store.set(key, value),
		};
	});
	afterEach(() => {
		delete (globalThis as any).localStorage;
	});

	it('round-trips and rejects junk', () => {
		expect(readLastCategory()).toBeNull();
		rememberCategory(12);
		expect(readLastCategory()).toBe(12);
		store.set(LAST_CATEGORY_KEY, 'abc');
		expect(readLastCategory()).toBeNull();
	});

	it('survives a missing localStorage', () => {
		delete (globalThis as any).localStorage;
		expect(readLastCategory()).toBeNull();
		expect(() => rememberCategory(1)).not.toThrow();
	});
});

describe('taskFromSelectionError', () => {
	it('explains selection_not_found and conflicts', () => {
		expect(
			taskFromSelectionError({
				response: { status: 422, data: { error: 'selection_not_found' } },
			}),
		).toContain('not found');
		expect(taskFromSelectionError({ name: 'PageConflictError' })).toContain(
			'changed',
		);
		expect(taskFromSelectionError({})).toBe('Failed to create task.');
	});
});
