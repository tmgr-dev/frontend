import {
	carriedSeparator,
	escapeCarriedHtml,
	joinCarriedMarkdown,
	prependCarriedBlock,
	splitTitleInsert,
	TITLE_LIMIT,
} from '../titleOverflow';

const filler = (n: number) => 'x'.repeat(n);

describe('splitTitleInsert', () => {
	it('leaves a fitting insert in the title', () => {
		expect(splitTitleInsert('abc', '', 'def')).toEqual({
			insert: 'def',
			overflow: '',
		});
		expect(splitTitleInsert(filler(254), '', 'a')).toEqual({
			insert: 'a',
			overflow: '',
		});
	});

	it('sends every typed character to the overflow when the title is full', () => {
		expect(splitTitleInsert(filler(TITLE_LIMIT), '', 'a')).toEqual({
			insert: '',
			overflow: 'a',
		});
	});

	it('cuts a paste at the last whitespace of the fitting part and drops it', () => {
		const result = splitTitleInsert(filler(250), '', 'ab cd efgh ij');
		expect(result).toEqual({ insert: 'ab cd', overflow: 'efgh ij' });
		expect(250 + result.insert.length).toBeLessThanOrEqual(TITLE_LIMIT);
	});

	it('keeps a word that ends exactly at the limit in the title', () => {
		expect(splitTitleInsert(filler(250), '', 'ab cd ef')).toEqual({
			insert: 'ab cd',
			overflow: 'ef',
		});
	});

	it('hard-cuts at the limit when the fitting part has no whitespace', () => {
		expect(splitTitleInsert(filler(250), '', 'abcdefghij klm')).toEqual({
			insert: 'abcde',
			overflow: 'fghij klm',
		});
	});

	it('never splits a surrogate pair on a hard cut', () => {
		const emoji = '\u{1F600}';
		const result = splitTitleInsert(filler(253), '', emoji.repeat(3));
		expect(result.insert).toBe(emoji);
		expect(result.overflow).toBe(emoji.repeat(2));
		expect(splitTitleInsert(filler(254), '', emoji)).toEqual({
			insert: '',
			overflow: emoji,
		});
	});

	it('counts the text after the caret against the limit and keeps it in the title', () => {
		const result = splitTitleInsert(filler(100), filler(150), 'ab cdef');
		expect(result).toEqual({ insert: 'ab', overflow: 'cdef' });
		expect(splitTitleInsert(filler(100), filler(150), 'abcdefgh')).toEqual({
			insert: 'abcde',
			overflow: 'fgh',
		});
	});

	it('treats a replaced selection as removed text', () => {
		const before = filler(10);
		const after = filler(10);
		expect(splitTitleInsert(before, after, 'a'.repeat(235))).toEqual({
			insert: 'a'.repeat(235),
			overflow: '',
		});
		expect(splitTitleInsert(before, after, 'a'.repeat(240))).toEqual({
			insert: 'a'.repeat(235),
			overflow: 'a'.repeat(5),
		});
	});

	it('cuts at whitespace in the first position instead of splitting the next word', () => {
		expect(splitTitleInsert(filler(252), '', ' hello')).toEqual({
			insert: '',
			overflow: 'hello',
		});
	});

	it('moves the whole paste when the title has no room left', () => {
		expect(splitTitleInsert(filler(200), filler(55), 'hello world')).toEqual({
			insert: '',
			overflow: 'hello world',
		});
	});
});

describe('joinCarriedMarkdown', () => {
	it('keeps the existing description after a blank line', () => {
		expect(joinCarriedMarkdown('carried', 'existing\n\nmore')).toBe(
			'carried\n\nexisting\n\nmore',
		);
	});

	it('adds no separator to an empty or blank description', () => {
		expect(carriedSeparator('')).toBe('');
		expect(joinCarriedMarkdown('carried', '')).toBe('carried');
		expect(carriedSeparator(' \n')).toBe('');
	});
});

describe('Editor.js fallbacks', () => {
	it('escapes carried text for a paragraph block', () => {
		expect(escapeCarriedHtml('a < b & c > d\ne')).toBe(
			'a &lt; b &amp; c &gt; d<br>e',
		);
	});

	it('puts a new first paragraph before the existing blocks and keeps the other fields', () => {
		const doc = {
			time: 1,
			version: '2.30.6',
			blocks: [{ type: 'header', data: { text: 'H', level: 2 } }],
		};
		expect(prependCarriedBlock(doc, 'a < b')).toEqual({
			time: 1,
			version: '2.30.6',
			blocks: [
				{ type: 'paragraph', data: { text: 'a &lt; b' } },
				{ type: 'header', data: { text: 'H', level: 2 } },
			],
		});
		expect(doc.blocks).toHaveLength(1);
	});

	it('creates a document when there is none and prepends to a legacy HTML string', () => {
		expect(prependCarriedBlock(null, 'x')).toEqual({
			blocks: [{ type: 'paragraph', data: { text: 'x' } }],
		});
		expect(prependCarriedBlock('<p>Existing</p>', 'x & y')).toBe(
			'<p>x &amp; y</p><p>Existing</p>',
		);
		expect(prependCarriedBlock('', 'x')).toBe('<p>x</p>');
	});
});
