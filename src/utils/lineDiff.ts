export type DiffLine = { type: 'same' | 'add' | 'del'; text: string };

const MAX_LCS_CELLS = 1_000_000;

const split = (text: string): string[] =>
	text === '' ? [] : text.replace(/\r\n/g, '\n').split('\n');

const middleDiff = (a: string[], b: string[]): DiffLine[] => {
	if ((a.length + 1) * (b.length + 1) > MAX_LCS_CELLS) {
		return [
			...a.map((text): DiffLine => ({ type: 'del', text })),
			...b.map((text): DiffLine => ({ type: 'add', text })),
		];
	}
	const lcs: number[][] = Array.from({ length: a.length + 1 }, () =>
		new Array(b.length + 1).fill(0),
	);
	for (let i = a.length - 1; i >= 0; i--) {
		for (let j = b.length - 1; j >= 0; j--) {
			lcs[i][j] =
				a[i] === b[j]
					? lcs[i + 1][j + 1] + 1
					: Math.max(lcs[i + 1][j], lcs[i][j + 1]);
		}
	}
	const out: DiffLine[] = [];
	let i = 0;
	let j = 0;
	while (i < a.length && j < b.length) {
		if (a[i] === b[j]) {
			out.push({ type: 'same', text: a[i] });
			i++;
			j++;
		} else if (lcs[i + 1][j] >= lcs[i][j + 1]) {
			out.push({ type: 'del', text: a[i++] });
		} else {
			out.push({ type: 'add', text: b[j++] });
		}
	}
	while (i < a.length) out.push({ type: 'del', text: a[i++] });
	while (j < b.length) out.push({ type: 'add', text: b[j++] });
	return out;
};

export const lineDiff = (before: string, after: string): DiffLine[] => {
	const a = split(before);
	const b = split(after);
	let start = 0;
	while (start < a.length && start < b.length && a[start] === b[start]) start++;
	let endA = a.length;
	let endB = b.length;
	while (endA > start && endB > start && a[endA - 1] === b[endB - 1]) {
		endA--;
		endB--;
	}
	const same = (lines: string[]): DiffLine[] =>
		lines.map((text): DiffLine => ({ type: 'same', text }));
	return [
		...same(a.slice(0, start)),
		...middleDiff(a.slice(start, endA), b.slice(start, endB)),
		...same(a.slice(endA)),
	];
};
