import { renderDoc } from '../docsMarkdown';

const render = (md: string) => {
	const doc = renderDoc(md, { copyLabel: 'Copy' });
	return doc.segments.map((s) => (s.type === 'html' ? s.html : '')).join('');
};

const fence = (lang: string, code: string) => `\`\`\`${lang}\n${code}\n\`\`\``;

// What the clipboard gets: the text of the code span without markup.
const codeText = (html: string) => {
	const inner = html.match(
		/<span class="md-editor-code-block">([\s\S]*?)<\/span><span rn-wrapper/,
	)![1];
	return inner
		.replace(/<\/?span[^>]*>/g, '')
		.replace(/&lt;/g, '<')
		.replace(/&gt;/g, '>')
		.replace(/&quot;/g, '"')
		.replace(/&amp;/g, '&');
};

const lineSpans = (html: string) =>
	html.match(/<span rn-wrapper aria-hidden="true">((?:<span><\/span>)*)</)![1]
		.length / '<span></span>'.length;

describe('renderDoc code blocks (md-editor-v3 markup)', () => {
	it.each(['sh', 'bash', 'shell', 'zsh', 'json', 'yaml'])(
		'%s: highlights with hljs spans and the library chrome',
		(lang) => {
			const code =
				lang === 'json'
					? '{"a": 1, "b": "x"}'
					: lang === 'yaml'
						? 'key: value\nlist:\n  - 1'
						: 'tmgr task list --json # all\necho "$HOME"';
			const html = render(fence(lang, code));
			expect(html).toContain('<div class="md-editor-code">');
			expect(html).toContain('<div class="md-editor-code-head">');
			expect(html).toContain(
				'<div class="md-editor-code-flag"><span></span><span></span><span></span></div>',
			);
			expect(html).toContain(
				`<span class="md-editor-code-lang">${lang}</span>`,
			);
			expect(html).toContain(
				'<button type="button" class="md-editor-copy-button" data-docs-copy>Copy</button>',
			);
			expect(html).toContain(
				`<code class="language-${lang}" language="${lang}">`,
			);
			expect(html).toContain('<span class="md-editor-code-block">');
			expect(html).toMatch(/<span class="hljs-/);
			expect(codeText(html)).toBe(code);
			expect(lineSpans(html)).toBe(code.split('\n').length);
		},
	);

	it('numbers a single line and an empty fence', () => {
		expect(lineSpans(render(fence('sh', 'ls')))).toBe(1);
		const empty = render('```sh\n```');
		expect(lineSpans(empty)).toBe(1);
		expect(codeText(empty)).toBe('');
	});

	it('keeps unknown or missing languages as escaped text', () => {
		const code = '<script>alert(1)</script> & "x"';
		for (const lang of ['foo', '']) {
			const html = render(fence(lang, code));
			expect(html).not.toContain('<script>');
			expect(html).not.toContain('hljs-');
			expect(html).toContain('&lt;script&gt;alert(1)&lt;/script&gt;');
			expect(codeText(html)).toBe(code);
			expect(html).toContain('<div class="md-editor-code">');
		}
		expect(render(fence('', 'ls'))).toContain('<code><span');
	});

	it('escapes markup inside highlighted languages and the label', () => {
		const html = render(fence('sh', 'echo "<img src=x onerror=1>"'));
		expect(html).not.toContain('<img');
		const label = renderDoc(fence('sh', 'ls'), {
			copyLabel: '<b>"</b>',
		}).segments[0];
		expect(label.type === 'html' && label.html).toContain(
			'&lt;b&gt;&quot;&lt;/b&gt;</button>',
		);
	});

	it('does not leak sibling blocks into each other', () => {
		const html = render(`${fence('sh', 'a')}\n\n${fence('json', '{}')}`);
		expect(html.match(/md-editor-code-head/g)).toHaveLength(2);
	});
});
