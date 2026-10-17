import {
	SNIPPET_BUTTON_CLASS,
	SNIPPET_ROW_CLASS,
	snippetTextClass,
} from '../snippetLayout';

describe('snippetLayout', () => {
	it('wrapped text breaks anywhere and never scrolls', () => {
		const cls = snippetTextClass(true).split(' ');
		expect(cls).toEqual(
			expect.arrayContaining([
				'whitespace-pre-wrap',
				'[overflow-wrap:anywhere]',
				'min-w-0',
			]),
		);
		expect(cls).not.toContain('overflow-x-auto');
	});

	it('code text scrolls horizontally without wrapping', () => {
		const cls = snippetTextClass(false).split(' ');
		expect(cls).toEqual(
			expect.arrayContaining(['whitespace-pre', 'overflow-x-auto', 'min-w-0']),
		);
	});

	it('keeps the copy button outside the text box, in the same flex row', () => {
		expect(SNIPPET_ROW_CLASS).toContain('sm:flex-row');
		expect(SNIPPET_BUTTON_CLASS).toContain('shrink-0');
		expect(SNIPPET_ROW_CLASS + SNIPPET_BUTTON_CLASS).not.toContain('absolute');
	});
});
