import type { UiNode } from '@/pluginSystem/uiTree';
import {
	compareSortValues,
	nextSort,
	nodeSortText,
	sortedIndexes,
	sparklinePoints,
} from '../pluginData';

const text = (t: string): UiNode => ({
	type: 'text',
	text: t,
	tone: 'default',
});
const row = (a: string, sort?: Record<string, string | number>) => ({
	cells: { a: text(a) },
	...(sort ? { sort } : {}),
});

describe('nextSort', () => {
	it('cycles asc, desc, none and restarts on another column', () => {
		const asc = nextSort(null, 'a');
		expect(asc).toEqual({ key: 'a', dir: 'asc' });
		const desc = nextSort(asc, 'a');
		expect(desc).toEqual({ key: 'a', dir: 'desc' });
		expect(nextSort(desc, 'a')).toBeNull();
		expect(nextSort(desc, 'b')).toEqual({ key: 'b', dir: 'asc' });
	});
});

describe('compareSortValues', () => {
	it('compares numbers numerically and text naturally', () => {
		expect(compareSortValues(2, 10)).toBeLessThan(0);
		expect(compareSortValues('item 2', 'item 10')).toBeLessThan(0);
		expect(compareSortValues('b', 'A')).toBeGreaterThan(0);
	});

	it('puts numbers before text', () => {
		expect(compareSortValues(5, 'a')).toBeLessThan(0);
		expect(compareSortValues('a', 5)).toBeGreaterThan(0);
	});
});

describe('nodeSortText', () => {
	it('reads the visible text of simple nodes', () => {
		expect(nodeSortText(text('hi'))).toBe('hi');
		expect(nodeSortText(undefined)).toBe('');
		expect(nodeSortText({ type: 'divider' })).toBe('');
	});
});

describe('sortedIndexes', () => {
	const rows = [row('b'), row('a'), row('c')];
	it('keeps the order without a sort', () => {
		expect(sortedIndexes(rows, null)).toEqual([0, 1, 2]);
	});

	it('sorts ascending and descending by cell text', () => {
		expect(sortedIndexes(rows, { key: 'a', dir: 'asc' })).toEqual([1, 0, 2]);
		expect(sortedIndexes(rows, { key: 'a', dir: 'desc' })).toEqual([2, 0, 1]);
	});

	it('prefers the row sort value over the cell text', () => {
		const withSort = [
			row('a', { a: 3 }),
			row('b', { a: 1 }),
			row('c', { a: 2 }),
		];
		expect(sortedIndexes(withSort, { key: 'a', dir: 'asc' })).toEqual([
			1, 2, 0,
		]);
	});

	it('is stable for equal values in both directions', () => {
		const same = [row('x'), row('x'), row('x')];
		expect(sortedIndexes(same, { key: 'a', dir: 'asc' })).toEqual([0, 1, 2]);
		expect(sortedIndexes(same, { key: 'a', dir: 'desc' })).toEqual([0, 1, 2]);
	});

	it('does not mutate the rows', () => {
		const copy = [...rows];
		sortedIndexes(rows, { key: 'a', dir: 'asc' });
		expect(rows).toEqual(copy);
	});
});

describe('sparklinePoints', () => {
	it('is empty without values', () => {
		expect(sparklinePoints([])).toBe('');
	});

	it('draws a single value as a flat line', () => {
		expect(sparklinePoints([7])).toBe('0,12 100,12');
	});

	it('draws a flat series in the middle', () => {
		expect(sparklinePoints([5, 5, 5])).toBe('0,12 50,12 100,12');
	});

	it('maps the minimum to the bottom and the maximum to the top', () => {
		expect(sparklinePoints([0, 10])).toBe('0,22 100,2');
	});

	it('handles negative values', () => {
		expect(sparklinePoints([-10, 0, 10])).toBe('0,22 50,12 100,2');
	});
});
