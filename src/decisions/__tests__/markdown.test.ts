/** @jest-environment jsdom */
import { clickedHttpLink, isAgentMessage, renderMarkdown } from '../markdown';

const parse = (html: string): HTMLElement => {
	const root = document.createElement('div');
	root.innerHTML = html;
	return root;
};

const clickOn = (html: string): MouseEvent => {
	const root = document.createElement('div');
	root.innerHTML = html;
	document.body.appendChild(root);
	const event = new MouseEvent('click', { bubbles: true, cancelable: true });
	Object.defineProperty(event, 'target', {
		value: root.querySelector('a') ?? root,
	});
	return event;
};

describe('renderMarkdown', () => {
	it('renders headings, lists, code, tables and links', () => {
		const html = renderMarkdown(
			[
				'# Title',
				'',
				'- one',
				'- two',
				'',
				'`inline` and',
				'',
				'```',
				'block',
				'```',
				'',
				'| a | b |',
				'|---|---|',
				'| 1 | 2 |',
				'',
				'[site](https://example.com)',
			].join('\n'),
		);
		expect(html).toContain('<h1');
		expect(html).toContain('<li>one</li>');
		expect(html).toContain('<code>inline</code>');
		expect(html).toContain('<pre>');
		expect(html).toContain('<table>');
		expect(html).toContain('href="https://example.com"');
	});

	it('strips scripts, handlers and javascript: links', () => {
		const html = renderMarkdown(
			'<script>alert(1)</script>\n\n<img src=x onerror=alert(1)>\n\n[x](javascript:alert(1))',
		);
		const root = parse(html);
		expect(root.querySelector('script')).toBeNull();
		expect(root.querySelector('[onerror]')).toBeNull();
		expect(root.querySelector('[href^="javascript:"]')).toBeNull();
	});

	it('shows raw HTML as literal text instead of dropping or running it', () => {
		const root = parse(
			renderMarkdown(
				'Use Array<string> and the <template> tag\n\n<form action="https://evil.example"><button>Approve</button></form>',
			),
		);
		expect(root.textContent).toContain('Array<string>');
		expect(root.textContent).toContain('<template> tag');
		expect(root.querySelector('form, button, textarea, template')).toBeNull();
		expect(root.textContent).toContain('<button>Approve</button>');
	});

	it('shows img markup as visible text in messages and context, without a live image', () => {
		for (const text of ['<img src=x>', '&lt;img src=x&gt;']) {
			const root = parse(renderMarkdown(text));
			expect(root.querySelector('img')).toBeNull();
			expect(root.textContent).toContain('<img src=x>');
			expect(root.textContent).not.toContain('&lt;');
		}
	});

	it('keeps code spans and fences literal', () => {
		const root = parse(renderMarkdown('`<b>x</b>`\n\n```\n<i>y</i>\n```'));
		expect(root.querySelector('b, i')).toBeNull();
		expect(root.textContent).toContain('<b>x</b>');
		expect(root.textContent).toContain('<i>y</i>');
	});

	it('blocks remote images', () => {
		const html = renderMarkdown('![pic](https://evil.example/p.png)');
		const root = parse(html);
		expect(root.querySelector('img')).toBeNull();
		expect(root.querySelector('a')?.getAttribute('href')).toBe(
			'https://evil.example/p.png',
		);
	});

	it('returns empty string for empty text', () => {
		expect(renderMarkdown('')).toBe('');
	});
});

describe('isAgentMessage', () => {
	it('treats only the owner as plain text', () => {
		expect(isAgentMessage({ author: 'agent' })).toBe(true);
		expect(isAgentMessage({ author: 'owner' })).toBe(false);
	});
});

describe('clickedHttpLink', () => {
	it('returns an http link and prevents navigation', () => {
		const event = clickOn('<a href="https://example.com/x">x</a>');
		expect(clickedHttpLink(event)).toBe('https://example.com/x');
		expect(event.defaultPrevented).toBe(true);
	});

	it('ignores javascript: links but still prevents navigation', () => {
		const event = clickOn('<a href="javascript:alert(1)">x</a>');
		expect(clickedHttpLink(event)).toBeNull();
		expect(event.defaultPrevented).toBe(true);
	});

	it('resolves a link when its child element is clicked', () => {
		const root = document.createElement('div');
		root.innerHTML = '<a href="https://example.com/y"><code>y</code></a>';
		const event = new MouseEvent('click', { bubbles: true, cancelable: true });
		Object.defineProperty(event, 'target', {
			value: root.querySelector('code'),
		});
		expect(clickedHttpLink(event)).toBe('https://example.com/y');
	});

	it('ignores clicks outside links', () => {
		const event = clickOn('<span>text</span>');
		expect(clickedHttpLink(event)).toBeNull();
		expect(event.defaultPrevented).toBe(false);
	});
});
