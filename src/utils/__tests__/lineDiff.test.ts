import { lineDiff } from '../lineDiff';

describe('lineDiff', () => {
	it('marks identical inputs as all same', () => {
		expect(lineDiff('a\nb', 'a\nb')).toEqual([
			{ type: 'same', text: 'a' },
			{ type: 'same', text: 'b' },
		]);
	});

	it('returns nothing for two empty strings', () => {
		expect(lineDiff('', '')).toEqual([]);
	});

	it('treats an empty old text as all added', () => {
		expect(lineDiff('', 'a\nb')).toEqual([
			{ type: 'add', text: 'a' },
			{ type: 'add', text: 'b' },
		]);
	});

	it('treats an empty new text as all deleted', () => {
		expect(lineDiff('a\nb', '')).toEqual([
			{ type: 'del', text: 'a' },
			{ type: 'del', text: 'b' },
		]);
	});

	it('reports a changed middle line as del then add', () => {
		expect(lineDiff('a\nb\nc', 'a\nx\nc')).toEqual([
			{ type: 'same', text: 'a' },
			{ type: 'del', text: 'b' },
			{ type: 'add', text: 'x' },
			{ type: 'same', text: 'c' },
		]);
	});

	it('shows a trailing newline difference', () => {
		expect(lineDiff('a', 'a\n')).toEqual([
			{ type: 'same', text: 'a' },
			{ type: 'add', text: '' },
		]);
		expect(lineDiff('a\n', 'a')).toEqual([
			{ type: 'same', text: 'a' },
			{ type: 'del', text: '' },
		]);
		expect(lineDiff('a\n', 'a\n')).toEqual([
			{ type: 'same', text: 'a' },
			{ type: 'same', text: '' },
		]);
	});

	it('normalizes CRLF', () => {
		expect(lineDiff('a\r\nb', 'a\nb')).toEqual([
			{ type: 'same', text: 'a' },
			{ type: 'same', text: 'b' },
		]);
	});

	it('handles insertions and deletions at the start', () => {
		expect(lineDiff('b\nc', 'a\nb')).toEqual([
			{ type: 'add', text: 'a' },
			{ type: 'same', text: 'b' },
			{ type: 'del', text: 'c' },
		]);
	});

	it('diffs large inputs quickly with a single change', () => {
		const lines = Array.from({ length: 8000 }, (_, i) => `l${i}`);
		const changed = [...lines];
		changed[4000] = 'x';
		const started = Date.now();
		const out = lineDiff(lines.join('\n'), changed.join('\n'));
		expect(Date.now() - started).toBeLessThan(1000);
		expect(out.filter((l) => l.type === 'del')).toEqual([
			{ type: 'del', text: 'l4000' },
		]);
		expect(out.filter((l) => l.type === 'add')).toEqual([
			{ type: 'add', text: 'x' },
		]);
		expect(out).toHaveLength(8001);
	});

	it('falls back to delete-all then add-all when the middle is too large', () => {
		const a = Array.from({ length: 8000 }, (_, i) => `a${i}`);
		const b = Array.from({ length: 8000 }, (_, i) => `b${i}`);
		a.unshift('head');
		b.unshift('head');
		a.push('tail');
		b.push('tail');
		const started = Date.now();
		const out = lineDiff(a.join('\n'), b.join('\n'));
		expect(Date.now() - started).toBeLessThan(1000);
		expect(out[0]).toEqual({ type: 'same', text: 'head' });
		expect(out[out.length - 1]).toEqual({ type: 'same', text: 'tail' });
		expect(out.slice(1, 8001).every((l) => l.type === 'del')).toBe(true);
		expect(out.slice(8001, 16001).every((l) => l.type === 'add')).toBe(true);
		expect(out).toHaveLength(16002);
	});
});
