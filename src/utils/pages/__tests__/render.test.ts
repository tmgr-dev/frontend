import { renderPageMarkdown } from '../render';

describe('renderPageMarkdown', () => {
	it('renders a tmgr link as an in-app chip without target', () => {
		const html = renderPageMarkdown('[TM-1](tmgr://task/1)');
		expect(html).toContain('data-tmgr="tmgr://task/1"');
		expect(html).toContain('class="tmgr-chip"');
		expect(html).toContain('href="#"');
		expect(html).not.toContain('target=');
	});

	it('keeps external links outward', () => {
		const html = renderPageMarkdown('[x](https://example.com)');
		expect(html).toContain('href="https://example.com"');
		expect(html).toContain('target="_blank"');
	});

	it('renders a tmgr file image as a placeholder with no src', () => {
		const html = renderPageMarkdown('![shot](tmgr://file/3)');
		expect(html).toContain('data-tmgr-file="3"');
		expect(html).not.toContain('src=');
	});

	it('does not emit a javascript: href', () => {
		const html = renderPageMarkdown('[x](javascript:alert(1))');
		expect(html).not.toContain('javascript:');
	});

	it('leaves a deep link as a chip too', () => {
		expect(renderPageMarkdown('[P](tmgr://page/ws/slug)')).toContain(
			'data-tmgr="tmgr://page/ws/slug"',
		);
	});

	it('is empty for empty input', () => {
		expect(renderPageMarkdown('')).toBe('');
	});
});
