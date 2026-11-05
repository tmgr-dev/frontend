/** @jest-environment jsdom */
import { clickedHttpLink, isAgentMessage, renderMarkdown } from '../markdown';

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
		expect(html).not.toContain('<script');
		expect(html).not.toContain('onerror');
		expect(html).not.toContain('javascript:');
	});

	it('blocks remote images', () => {
		const html = renderMarkdown('![pic](https://evil.example/p.png)');
		expect(html).not.toContain('<img');
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

	it('ignores clicks outside links', () => {
		const event = clickOn('<span>text</span>');
		expect(clickedHttpLink(event)).toBeNull();
		expect(event.defaultPrevented).toBe(false);
	});
});
