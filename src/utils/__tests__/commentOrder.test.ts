import { sortCommentsOldestFirst } from '../commentOrder';

const c = (id: number, created_at: string) => ({ id, created_at });

describe('sortCommentsOldestFirst', () => {
	it('reads like a conversation: the newest comment is last', () => {
		const sorted = sortCommentsOldestFirst([
			c(2, '2026-09-12T10:00:00Z'),
			c(1, '2026-09-11T10:00:00Z'),
			c(3, '2026-09-12T18:00:00Z'),
		]);
		expect(sorted.map((x) => x.id)).toEqual([1, 2, 3]);
	});

	it('leaves the array it was given untouched', () => {
		const input = [c(2, '2026-09-12T10:00:00Z'), c(1, '2026-09-11T10:00:00Z')];
		sortCommentsOldestFirst(input);
		expect(input.map((x) => x.id)).toEqual([2, 1]);
	});

	it('keeps comments written in the same second in the order they arrived', () => {
		const same = '2026-09-12T10:00:00Z';
		const sorted = sortCommentsOldestFirst([c(7, same), c(8, same), c(9, same)]);
		expect(sorted.map((x) => x.id)).toEqual([7, 8, 9]);
	});

	it('puts a comment with no timestamp first instead of dropping it', () => {
		const sorted = sortCommentsOldestFirst([
			c(2, '2026-09-12T10:00:00Z'),
			{ id: 1, created_at: '' },
		]);
		expect(sorted.map((x) => x.id)).toEqual([1, 2]);
	});

	it('handles an empty list', () => {
		expect(sortCommentsOldestFirst([])).toEqual([]);
	});
});
