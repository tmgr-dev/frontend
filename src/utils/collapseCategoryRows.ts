export const CATEGORY_ROWS_LIMIT = 5;

interface DepthRow {
	depth: number;
}

export const collapseCategoryRows = <T extends DepthRow>(
	rows: T[],
	expanded: boolean,
	limit = CATEGORY_ROWS_LIMIT,
) => {
	const totalRoots = rows.filter((r) => r.depth === 0).length;
	if (totalRoots <= limit) {
		return { rows, hidden: 0, toggle: null as 'more' | 'less' | null };
	}
	if (expanded) {
		return { rows, hidden: 0, toggle: 'less' as const };
	}
	let seen = 0;
	let cut = rows.length;
	for (let i = 0; i < rows.length; i++) {
		if (rows[i].depth === 0 && ++seen > limit) {
			cut = i;
			break;
		}
	}
	return {
		rows: rows.slice(0, cut),
		hidden: totalRoots - limit,
		toggle: 'more' as const,
	};
};

export const rootIndexOf = <T extends DepthRow & { id?: number }>(
	rows: T[],
	id: number,
) => {
	let root = -1;
	for (const row of rows) {
		if (row.depth === 0) root++;
		if (row.id === id) return root;
	}
	return -1;
};
