import type { StarterVisibility } from '../starters';
import { showRemoveStarters } from '../starters';

const ctx = (over: Partial<StarterVisibility> = {}): StarterVisibility => ({
	intent: 'human',
	hasUser: true,
	removed: false,
	boardTasks: false,
	boardHasStarter: false,
	expected: true,
	...over,
});

describe('showRemoveStarters', () => {
	it('shows on the hub while seeded starters are expected', () => {
		expect(showRemoveStarters(ctx())).toBe(true);
	});

	it.each([
		['nothing expected', { expected: false }],
		['already removed', { removed: true }],
		['no user', { hasUser: false }],
		['skipped intent', { intent: 'skipped' }],
		['null intent', { intent: null }],
		[
			'board without a starter task',
			{ boardTasks: true, boardHasStarter: false },
		],
	])('hides: %s', (_name, over) => {
		expect(showRemoveStarters(ctx(over as Partial<StarterVisibility>))).toBe(
			false,
		);
	});

	it('shows on a board that holds a starter task even when no marker exists', () => {
		expect(
			showRemoveStarters(
				ctx({ boardTasks: true, boardHasStarter: true, expected: false }),
			),
		).toBe(true);
	});
});
