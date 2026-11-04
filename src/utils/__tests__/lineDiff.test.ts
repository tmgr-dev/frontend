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

	it('ignores a single trailing newline', () => {
		expect(lineDiff('a\n', 'a')).toEqual([{ type: 'same', text: 'a' }]);
		expect(lineDiff('a\nb\n', 'a\nb\nc\n')).toEqual([
			{ type: 'same', text: 'a' },
			{ type: 'same', text: 'b' },
			{ type: 'add', text: 'c' },
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
});
