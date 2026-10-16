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

	it('sizes an image from the Outline title slot', () => {
		const html = renderPageMarkdown('![](tmgr://file/3 " =300x200")');
		expect(html).toContain('data-tmgr-file="3"');
		expect(html).toContain('width="300"');
		expect(html).toContain('data-layout="center"');
		expect(html).not.toContain('page-image-caption');
	});

	it('renders layout and caption', () => {
		const html = renderPageMarkdown(
			'![A <b>cap</b>](https://example.com/a.png "right-50 =120x")',
		);
		expect(html).toContain('data-layout="right-50"');
		expect(html).toContain('width="120"');
		expect(html).toContain('src="https://example.com/a.png"');
		expect(html).toContain(
			'<span class="page-image-caption">A &lt;b&gt;cap&lt;/b&gt;</span>',
		);
	});

	it('does not show a legacy filename alt as a caption', () => {
		const html = renderPageMarkdown('![shot.png](tmgr://file/3)');
		expect(html).toContain('alt="shot.png"');
		expect(html).not.toContain('page-image-caption');
		expect(html).not.toContain('width=');
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
