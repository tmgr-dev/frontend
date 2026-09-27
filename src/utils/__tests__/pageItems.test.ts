import { pageItems } from '../pageItems';

it('shows every page when they fit', () => {
	expect(pageItems(1, 1)).toEqual([1]);
	expect(pageItems(3, 5)).toEqual([1, 2, 3, 4, 5]);
});

it('keeps the first, the last and the neighbours of the current page', () => {
	expect(pageItems(1, 23)).toEqual([1, 2, 3, '…', 23]);
	expect(pageItems(3, 23)).toEqual([1, 2, 3, 4, '…', 23]);
	expect(pageItems(12, 23)).toEqual([1, '…', 11, 12, 13, '…', 23]);
	expect(pageItems(21, 23)).toEqual([1, '…', 20, 21, 22, 23]);
	expect(pageItems(23, 23)).toEqual([1, '…', 21, 22, 23]);
});

it('never hides a single page behind an ellipsis', () => {
	expect(pageItems(4, 23)).toEqual([1, 2, 3, 4, 5, '…', 23]);
	expect(pageItems(20, 23)).toEqual([1, '…', 19, 20, 21, 22, 23]);
});
