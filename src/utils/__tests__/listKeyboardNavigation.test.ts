import {
	nextFocusIndex,
	shouldIgnoreNavigationTarget,
} from '../listKeyboardNavigation';

describe('nextFocusIndex', () => {
	it('returns -1 for an empty list', () => {
		expect(nextFocusIndex(-1, 0, 1)).toBe(-1);
		expect(nextFocusIndex(2, 0, -1)).toBe(-1);
	});

	it('selects the first row on ArrowDown when nothing is focused', () => {
		expect(nextFocusIndex(-1, 5, 1)).toBe(0);
	});

	it('selects the last row on ArrowUp when nothing is focused', () => {
		expect(nextFocusIndex(-1, 5, -1)).toBe(4);
	});

	it('moves down and up within bounds', () => {
		expect(nextFocusIndex(0, 5, 1)).toBe(1);
		expect(nextFocusIndex(3, 5, -1)).toBe(2);
	});

	it('stops at the bottom without wrapping', () => {
		expect(nextFocusIndex(4, 5, 1)).toBe(4);
	});

	it('stops at the top without wrapping', () => {
		expect(nextFocusIndex(0, 5, -1)).toBe(0);
	});

	it('clamps an out-of-range index back into the list', () => {
		expect(nextFocusIndex(99, 5, 1)).toBe(0);
		expect(nextFocusIndex(99, 5, -1)).toBe(4);
	});
});

describe('shouldIgnoreNavigationTarget', () => {
	const el = (tag: string, extra: Record<string, unknown> = {}) =>
		({ tagName: tag, closest: () => null, ...extra }) as unknown as EventTarget;

	it('ignores typing in text fields', () => {
		expect(shouldIgnoreNavigationTarget(el('INPUT'))).toBe(true);
		expect(shouldIgnoreNavigationTarget(el('TEXTAREA'))).toBe(true);
		expect(shouldIgnoreNavigationTarget(el('SELECT'))).toBe(true);
	});

	it('ignores contenteditable elements', () => {
		expect(
			shouldIgnoreNavigationTarget(el('DIV', { isContentEditable: true })),
		).toBe(true);
	});

	it('ignores elements inside an editor surface', () => {
		const inEditor = {
			tagName: 'SPAN',
			closest: (sel: string) => (sel.includes('codex-editor') ? {} : null),
		} as unknown as EventTarget;
		expect(shouldIgnoreNavigationTarget(inEditor)).toBe(true);
	});

	it('does not ignore plain elements like buttons or the body', () => {
		expect(shouldIgnoreNavigationTarget(el('BUTTON'))).toBe(false);
		expect(shouldIgnoreNavigationTarget(el('DIV'))).toBe(false);
	});

	it('returns false for a null target', () => {
		expect(shouldIgnoreNavigationTarget(null)).toBe(false);
	});
});
