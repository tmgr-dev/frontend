import type { ColumnAlign, ColumnWidth, UiNode } from '@/pluginSystem/uiTree';

export type SortDir = 'asc' | 'desc';
export interface SortState {
	key: string;
	dir: SortDir;
}

export const COLUMN_ALIGN_CLASS: Record<ColumnAlign, string> = {
	left: 'text-left',
	center: 'text-center',
	right: 'text-right',
};

export const COLUMN_WIDTH_CLASS: Record<ColumnWidth, string> = {
	xs: 'w-16 min-w-16',
	sm: 'w-28 min-w-28',
	md: 'w-48 min-w-48',
	lg: 'w-72 min-w-72',
};

export const nextSort = (
	current: SortState | null,
	key: string,
): SortState | null => {
	if (current?.key !== key) return { key, dir: 'asc' };
	return current.dir === 'asc' ? { key, dir: 'desc' } : null;
};

export const compareSortValues = (
	a: string | number,
	b: string | number,
): number => {
	if (typeof a === 'number' && typeof b === 'number') return a - b;
	if (typeof a === 'number') return -1;
	if (typeof b === 'number') return 1;
	return a.localeCompare(b, undefined, { numeric: true, sensitivity: 'base' });
};

export const nodeSortText = (node: UiNode | undefined): string => {
	if (!node) return '';
	switch (node.type) {
		case 'text':
		case 'badge':
		case 'taskLink':
		case 'link':
		case 'copyable':
			return node.text;
		case 'heading':
			return node.text;
		case 'stat':
			return node.value;
		case 'timeAgo':
		case 'dueTime':
			return node.at;
		default:
			return '';
	}
};

export const sortedIndexes = <
	R extends {
		cells: Record<string, UiNode>;
		sort?: Record<string, string | number>;
	},
>(
	rows: R[],
	sort: SortState | null,
): number[] => {
	const indexes = rows.map((_, index) => index);
	if (!sort) return indexes;
	const valueOf = (row: R): string | number =>
		row.sort?.[sort.key] ?? nodeSortText(row.cells[sort.key]);
	const sign = sort.dir === 'asc' ? 1 : -1;
	return indexes.sort(
		(a, b) =>
			sign * compareSortValues(valueOf(rows[a]), valueOf(rows[b])) || a - b,
	);
};

export const SPARKLINE_WIDTH = 100;
export const SPARKLINE_HEIGHT = 24;
const SPARKLINE_PAD = 2;

export const sparklinePoints = (values: number[]): string => {
	if (values.length === 0) return '';
	const mid = SPARKLINE_HEIGHT / 2;
	const round = (n: number) => Math.round(n * 100) / 100;
	if (values.length === 1) return `0,${mid} ${SPARKLINE_WIDTH},${mid}`;
	const min = Math.min(...values);
	const max = Math.max(...values);
	const span = max - min;
	const inner = SPARKLINE_HEIGHT - SPARKLINE_PAD * 2;
	return values
		.map((v, i) => {
			const x = (i / (values.length - 1)) * SPARKLINE_WIDTH;
			const y =
				span === 0 ? mid : SPARKLINE_PAD + inner - ((v - min) / span) * inner;
			return `${round(x)},${round(y)}`;
		})
		.join(' ');
};
