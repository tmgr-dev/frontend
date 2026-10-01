export interface DiffOp {
	type: 'same' | 'add' | 'del';
	text: string;
	oldLine: number | null;
	newLine: number | null;
}

const CELL_BUDGET = 4_000_000;

const split = (value: string): string[] =>
	value === '' ? [] : value.replace(/\r\n/g, '\n').split('\n');

export const diffLines = (before: string, after: string): DiffOp[] => {
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

	const ops: DiffOp[] = [];
	let oldLine = 1;
	let newLine = 1;
	const push = (type: DiffOp['type'], text: string) => {
		ops.push({
			type,
			text,
			oldLine: type === 'add' ? null : oldLine,
			newLine: type === 'del' ? null : newLine,
		});
		if (type !== 'add') oldLine++;
		if (type !== 'del') newLine++;
	};

	for (let i = 0; i < start; i++) push('same', a[i]);

	const n = endA - start;
	const m = endB - start;
	if (n * m > CELL_BUDGET) {
		for (let i = start; i < endA; i++) push('del', a[i]);
		for (let j = start; j < endB; j++) push('add', b[j]);
	} else {
		const width = m + 1;
		const table = new Uint32Array((n + 1) * width);
		for (let i = n - 1; i >= 0; i--) {
			for (let j = m - 1; j >= 0; j--) {
				table[i * width + j] =
					a[start + i] === b[start + j]
						? table[(i + 1) * width + j + 1] + 1
						: Math.max(table[(i + 1) * width + j], table[i * width + j + 1]);
			}
		}
		let i = 0;
		let j = 0;
		while (i < n || j < m) {
			if (i < n && j < m && a[start + i] === b[start + j]) {
				push('same', a[start + i]);
				i++;
				j++;
			} else if (
				i < n &&
				(j >= m || table[(i + 1) * width + j] >= table[i * width + j + 1])
			) {
				push('del', a[start + i]);
				i++;
			} else {
				push('add', b[start + j]);
				j++;
			}
		}
	}

	for (let i = endA; i < a.length; i++) push('same', a[i]);
	return ops;
};

export const diffStats = (
	ops: DiffOp[],
): { added: number; removed: number } => ({
	added: ops.filter((o) => o.type === 'add').length,
	removed: ops.filter((o) => o.type === 'del').length,
});
