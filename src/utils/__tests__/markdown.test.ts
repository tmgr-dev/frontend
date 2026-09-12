import { markdownToHtml } from '../markdown';

describe('markdownToHtml', () => {
	it('renders the formatting people actually use in comments', () => {
		const html = markdownToHtml('**bold** and `code`');
		expect(html).toContain('<strong>bold</strong>');
		expect(html).toContain('<code>code</code>');
	});

	it('renders lists', () => {
		const html = markdownToHtml('- one\n- two');
		expect(html).toContain('<li>one</li>');
		expect(html).toContain('<li>two</li>');
	});

	it('keeps a single newline as a line break, the way a comment is typed', () => {
		expect(markdownToHtml('first\nsecond')).toContain('<br>');
	});

	it('sends links to a new tab without handing it a window opener', () => {
		const html = markdownToHtml('[docs](https://example.com)');
		expect(html).toContain('href="https://example.com"');
		expect(html).toContain('target="_blank"');
		expect(html).toContain('rel="noopener noreferrer"');
	});

	it('returns nothing for empty input', () => {
		expect(markdownToHtml('')).toBe('');
		expect(markdownToHtml(null as unknown as string)).toBe('');
		expect(markdownToHtml(undefined as unknown as string)).toBe('');
	});

	it('leaves plain text readable', () => {
		expect(markdownToHtml('just a sentence')).toContain('just a sentence');
	});

	it('passes raw HTML through for the sanitizer to deal with, not the parser', () => {
		// marked does not strip HTML; the component runs DOMPurify over the result,
		// so this test pins where that responsibility lives.
		expect(markdownToHtml('<img src=x onerror=alert(1)>')).toContain('onerror');
	});
});
