export type PageItem = number | '…';

/**
 * Page buttons for a narrow screen: the first and last page, the current one with its neighbours, and an
 * ellipsis for each gap. A gap of a single page shows that page instead, since the ellipsis would be as wide.
 */
export const pageItems = (current: number, last: number): PageItem[] => {
	if (last <= 1) return [1];
	const start = Math.min(Math.max(current - 1, 1), Math.max(last - 2, 1));
	const shown = new Set([1, last]);
	for (let page = start; page <= Math.min(start + 2, last); page++)
		shown.add(page);
	const pages = [...shown].sort((a, b) => a - b);
	const items: PageItem[] = [];
	pages.forEach((page, i) => {
		const previous = pages[i - 1];
		if (previous !== undefined && page - previous === 2) items.push(page - 1);
		else if (previous !== undefined && page - previous > 2) items.push('…');
		items.push(page);
	});
	return items;
};
