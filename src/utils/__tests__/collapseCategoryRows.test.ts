import {
	CATEGORY_ROWS_LIMIT,
	collapseCategoryRows,
	rootIndexOf,
} from '../collapseCategoryRows';

const row = (id: number, depth = 0) => ({ id, depth });
const roots = (n: number) => Array.from({ length: n }, (_, i) => row(i + 1));

describe('collapseCategoryRows', () => {
	it('keeps everything when there are at most 5 roots', () => {
		const rows = roots(CATEGORY_ROWS_LIMIT);
		expect(collapseCategoryRows(rows, false)).toEqual({
			rows,
			hidden: 0,
			toggle: null,
		});
	});

	it('shows the first 5 roots and a Show more toggle', () => {
		const result = collapseCategoryRows(roots(8), false);
		expect(result.rows.map((r) => r.id)).toEqual([1, 2, 3, 4, 5]);
		expect(result.hidden).toBe(3);
		expect(result.toggle).toBe('more');
	});

	it('shows all rows and a Show less toggle when expanded', () => {
		const result = collapseCategoryRows(roots(8), true);
		expect(result.rows).toHaveLength(8);
		expect(result.toggle).toBe('less');
	});

	it('counts roots, not nested rows, and keeps subtrees whole', () => {
		const rows = [
			row(1),
			row(10, 1),
			row(11, 1),
			row(2),
			row(3),
			row(4),
			row(5),
			row(50, 1),
			row(6),
			row(7),
		];
		const result = collapseCategoryRows(rows, false);
		expect(result.rows.map((r) => r.id)).toEqual([1, 10, 11, 2, 3, 4, 5, 50]);
		expect(result.hidden).toBe(2);
	});

	it('finds the root index of a row', () => {
		const rows = [row(1), row(10, 1), row(2), row(3, 0), row(30, 1)];
		expect(rootIndexOf(rows, 30)).toBe(2);
		expect(rootIndexOf(rows, 10)).toBe(0);
		expect(rootIndexOf(rows, 99)).toBe(-1);
	});
});
