export type DiffLine = { type: 'same' | 'add' | 'del'; text: string };

const split = (text: string): string[] => {
	if (text === '') return [];
	const lines = text.replace(/\r\n/g, '\n').split('\n');
	if (lines[lines.length - 1] === '') lines.pop();
	return lines;
};

export const lineDiff = (before: string, after: string): DiffLine[] => {
	const a = split(before);
	const b = split(after);
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
