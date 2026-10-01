import { diffLines, diffStats } from '../lineDiff';

const text = (ops: ReturnType<typeof diffLines>) =>
	ops.map(
		(o) => `${o.type === 'add' ? '+' : o.type === 'del' ? '-' : ' '}${o.text}`,
	);

describe('diffLines', () => {
	it('marks identical text as all same', () => {
		expect(text(diffLines('a\nb', 'a\nb'))).toEqual([' a', ' b']);
	});

	it('detects added and removed lines', () => {
		expect(text(diffLines('a\nb\nc', 'a\nc\nd'))).toEqual([
			' a',
			'-b',
			' c',
			'+d',
		]);
	});

	it('handles an empty old text', () => {
		expect(text(diffLines('', 'x\ny'))).toEqual(['+x', '+y']);
	});

	it('handles an empty new text', () => {
		expect(text(diffLines('x\ny', ''))).toEqual(['-x', '-y']);
	});

	it('replaces a changed line as del then add', () => {
		expect(text(diffLines('a\nold\nz', 'a\nnew\nz'))).toEqual([
			' a',
			'-old',
			'+new',
			' z',
		]);
	});

	it('numbers old and new lines', () => {
		const ops = diffLines('a\nb', 'a\nc');
		expect(ops[0]).toMatchObject({ type: 'same', oldLine: 1, newLine: 1 });
		expect(ops[1]).toMatchObject({ type: 'del', oldLine: 2, newLine: null });
		expect(ops[2]).toMatchObject({ type: 'add', oldLine: null, newLine: 2 });
	});

	it('normalizes CRLF', () => {
		expect(text(diffLines('a\r\nb', 'a\nb'))).toEqual([' a', ' b']);
	});

	it('reconstructs both sides from the ops', () => {
		const a = 'one\ntwo\nthree\nfour\nfive';
		const b = 'one\n2\nthree\nfive\nsix';
		const ops = diffLines(a, b);
		expect(
			ops
				.filter((o) => o.type !== 'add')
				.map((o) => o.text)
				.join('\n'),
		).toBe(a);
		expect(
			ops
				.filter((o) => o.type !== 'del')
				.map((o) => o.text)
				.join('\n'),
		).toBe(b);
	});

	it('falls back to a block replace beyond the cell budget but still reconstructs', () => {
		const a = Array.from({ length: 3000 }, (_, i) => `a${i}`).join('\n');
		const b = Array.from({ length: 3000 }, (_, i) => `b${i}`).join('\n');
		const ops = diffLines(a, b);
		expect(
			ops
				.filter((o) => o.type !== 'add')
				.map((o) => o.text)
				.join('\n'),
		).toBe(a);
		expect(
			ops
				.filter((o) => o.type !== 'del')
				.map((o) => o.text)
				.join('\n'),
		).toBe(b);
	});
});

describe('diffStats', () => {
	it('counts additions and removals', () => {
		expect(diffStats(diffLines('a\nb', 'a\nc\nd'))).toEqual({
			added: 2,
			removed: 1,
		});
	});
});
