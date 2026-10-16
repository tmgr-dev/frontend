/** @jest-environment jsdom */
import editorJsToHtml from '../editorJsToHtml';

const render = (...blocks: unknown[]) => editorJsToHtml({ blocks });
const parse = (html: string) => {
	const root = document.createElement('div');
	root.innerHTML = html;
	return root;
};

describe('editorJsToHtml', () => {
	it.each([
		null,
		undefined,
		'',
		'not json',
		{},
		{ blocks: [] },
		{ blocks: null },
	])('returns an empty string for %p', (data) => {
		expect(editorJsToHtml(data)).toBe('');
	});

	it('accepts JSON strings', () => {
		expect(
			editorJsToHtml(
				JSON.stringify({
					blocks: [{ type: 'paragraph', data: { text: 'a' } }],
				}),
			),
		).toBe('<p>a</p>');
	});

	it('renders headers with a clamped level', () => {
		expect(render({ type: 'header', data: { text: 'T', level: 3 } })).toBe(
			'<h3>T</h3>',
		);
		expect(render({ type: 'header', data: { text: 'T', level: 9 } })).toBe(
			'<h6>T</h6>',
		);
		expect(render({ type: 'header', data: { text: 'T', level: 'x' } })).toBe(
			'<h2>T</h2>',
		);
	});

	it('keeps inline formatting in paragraphs', () => {
		const html = render({
			type: 'paragraph',
			data: {
				text: '<b>b</b> <code class="inline-code">c</code> <mark class="cdx-marker">m</mark> <a href="https://x.example">l</a>',
			},
		});
		const root = parse(html);
		expect(root.querySelector('p b')?.textContent).toBe('b');
		expect(root.querySelector('p code')?.textContent).toBe('c');
		expect(root.querySelector('p mark')?.textContent).toBe('m');
		expect(root.querySelector('p a')?.getAttribute('href')).toBe(
			'https://x.example',
		);
	});

	it('renders flat and nested lists', () => {
		expect(
			render({ type: 'list', data: { style: 'unordered', items: ['a', 'b'] } }),
		).toBe('<ul><li>a</li><li>b</li></ul>');
		expect(
			render({
				type: 'list',
				data: {
					style: 'ordered',
					items: [
						{ content: 'a', items: [{ content: 'a1', items: [] }] },
						{ content: 'b', items: [] },
					],
				},
			}),
		).toBe('<ol><li>a<ol><li>a1</li></ol></li><li>b</li></ol>');
	});

	it('renders checklist items as disabled checkboxes', () => {
		const root = parse(
			render({
				type: 'checklist',
				data: {
					items: [
						{ text: 'done', checked: true },
						{ text: 'todo', checked: false },
					],
				},
			}),
		);
		const boxes = root.querySelectorAll<HTMLInputElement>(
			'input[type=checkbox]',
		);
		expect(boxes).toHaveLength(2);
		expect([...boxes].map((box) => box.checked)).toEqual([true, false]);
		expect([...boxes].every((box) => box.disabled)).toBe(true);
		expect(root.textContent).toContain('done');
		expect(root.textContent).toContain('todo');
	});

	it('renders list@2 ordered, unordered and nested lists', () => {
		const item = (content: string, items: unknown[] = []) => ({
			content,
			meta: {},
			items,
		});
		expect(
			render({
				type: 'list',
				data: {
					style: 'ordered',
					meta: { counterType: 'numeric' },
					items: [item('a', [item('a1')]), item('b')],
				},
			}),
		).toBe('<ol><li>a<ol><li>a1</li></ol></li><li>b</li></ol>');
		expect(
			render({
				type: 'list',
				data: { style: 'unordered', meta: {}, items: [item('<b>x</b>')] },
			}),
		).toBe('<ul><li><b>x</b></li></ul>');
	});

	it('keeps a valid ordered list start through sanitize, top level only', () => {
		const ordered = (start: unknown) =>
			render({
				type: 'list',
				data: {
					style: 'ordered',
					meta: { start, counterType: 'numeric' },
					items: [{ content: 'a', meta: {}, items: [{ content: 'a1', meta: {}, items: [] }] }],
				},
			});
		expect(ordered(5)).toBe('<ol start="5"><li>a<ol><li>a1</li></ol></li></ol>');
		expect(parse(ordered(5)).querySelector('ol')!.start).toBe(5);
		for (const start of [1, 0, -2, 2.5, '7', '" onclick="x', undefined])
			expect(ordered(start)).toBe('<ol><li>a<ol><li>a1</li></ol></li></ol>');
	});

	it('renders list@2 checklists with nested items as disabled checkboxes', () => {
		const root = parse(
			render({
				type: 'list',
				data: {
					style: 'checklist',
					meta: {},
					items: [
						{
							content: 'parent',
							meta: { checked: true },
							items: [{ content: 'child', meta: { checked: false }, items: [] }],
						},
						{ content: 'todo', meta: { checked: false }, items: [] },
					],
				},
			}),
		);
		const outer = root.querySelector('ul.checklist')!;
		const nested = outer.querySelector('li ul.checklist')!;
		expect(nested.textContent).toBe(' child');
		const boxes = [...root.querySelectorAll<HTMLInputElement>('input[type=checkbox]')];
		expect(boxes.map((box) => box.checked)).toEqual([true, false, false]);
		expect(boxes.every((box) => box.disabled)).toBe(true);
		expect(root.textContent).toContain('todo');
	});

	it('renders nothing for empty or malformed lists', () => {
		expect(render({ type: 'list', data: { style: 'unordered', items: [] } })).toBe('');
		expect(render({ type: 'checklist', data: { items: [] } })).toBe('');
		expect(render({ type: 'checklist', data: { items: [null, 1] } })).toBe('');
		expect(render({ type: 'list', data: { style: 'checklist', meta: {}, items: [] } })).toBe('');
	});

	it('renders quote, warning, delimiter and code', () => {
		expect(render({ type: 'quote', data: { text: 'q', caption: 'c' } })).toBe(
			'<blockquote><p>q</p><figcaption>c</figcaption></blockquote>',
		);
		expect(
			render({ type: 'warning', data: { title: 't', message: 'm' } }),
		).toBe('<div class="warning"><strong>t</strong><p>m</p></div>');
		expect(render({ type: 'delimiter', data: {} })).toBe('<hr>');
		expect(render({ type: 'code', data: { code: '<b>x</b> && y' } })).toBe(
			'<pre><code>&lt;b&gt;x&lt;/b&gt; &amp;&amp; y</code></pre>',
		);
		expect(
			render({
				type: 'codebox',
				data: {
					code: '<span class="hljs-keyword">const</span> a = 1;<br>a &lt; 2<div>b &amp;&amp; c</div><div>d</div>',
				},
			}),
		).toBe('<pre><code>const a = 1;\na &lt; 2b &amp;&amp; c\nd</code></pre>');
	});

	it('strips page-restyling and form markup from raw and inline HTML', () => {
		const out = render(
			{
				type: 'raw',
				data: {
					html: '<style>body{display:none}</style><p style="position:fixed" class="fixed inset-0">x</p>',
				},
			},
			{
				type: 'paragraph',
				data: {
					text: '<form action="https://evil.example"><input type="password" name="p"><button>Go</button><textarea></textarea><select></select></form>ok',
				},
			},
		);
		const root = parse(out);
		expect(
			root.querySelector('style, form, button, textarea, select'),
		).toBeNull();
		expect(root.querySelector('[style]')).toBeNull();
		expect(root.querySelector('[class]')).toBeNull();
		expect(root.querySelector('input')).toBeNull();
		expect(root.textContent).toContain('ok');
	});

	it('renders tables with and without headings', () => {
		const content = [
			['h1', 'h2'],
			['a', 'b'],
		];
		expect(
			render({ type: 'table', data: { withHeadings: true, content } }),
		).toBe(
			'<table><thead><tr><th>h1</th><th>h2</th></tr></thead><tbody><tr><td>a</td><td>b</td></tr></tbody></table>',
		);
		expect(
			render({ type: 'table', data: { withHeadings: false, content } }),
		).toBe(
			'<table><tbody><tr><td>h1</td><td>h2</td></tr><tr><td>a</td><td>b</td></tr></tbody></table>',
		);
	});

	it('renders raw html, images and http(s) embeds', () => {
		expect(render({ type: 'raw', data: { html: '<em>r</em>' } })).toBe(
			'<em>r</em>',
		);
		const image = parse(
			render({
				type: 'image',
				data: { file: { url: 'https://x.example/a.png' }, caption: 'cap' },
			}),
		);
		expect(image.querySelector('img')?.getAttribute('src')).toBe(
			'https://x.example/a.png',
		);
		expect(image.querySelector('figcaption')?.textContent).toBe('cap');
		const embed = parse(
			render({
				type: 'embed',
				data: {
					service: 'youtube',
					source: 'https://youtube.com/watch?v=1',
					embed: 'https://youtube.com/embed/1',
				},
			}),
		);
		expect(embed.querySelector('iframe')).toBeNull();
		expect(embed.querySelector('a')?.getAttribute('href')).toBe(
			'https://youtube.com/watch?v=1',
		);
		expect(
			render({ type: 'embed', data: { source: 'javascript:alert(1)' } }),
		).toBe('');
	});

	it('skips unknown and malformed blocks', () => {
		expect(
			render({ type: 'mystery', data: { text: 'x' } }, null, 'str', {
				type: 'paragraph',
				data: { text: 'ok' },
			}),
		).toBe('<p>ok</p>');
	});

	it('strips XSS after rendering', () => {
		const root = parse(
			render(
				{
					type: 'paragraph',
					data: {
						text: '<img src=x onerror="alert(1)"><script>alert(1)</script>p',
					},
				},
				{
					type: 'raw',
					data: {
						html: '<a href="javascript:alert(1)">r</a><iframe src="https://x"></iframe>',
					},
				},
				{ type: 'image', data: { url: 'javascript:alert(1)' } },
				{ type: 'image', data: { url: 'x" onerror="alert(1)' } },
			),
		);
		expect(root.querySelector('script, iframe, [onerror]')).toBeNull();
		expect(root.innerHTML).not.toMatch(/javascript:/i);
		expect(root.textContent).toContain('p');
	});
});
