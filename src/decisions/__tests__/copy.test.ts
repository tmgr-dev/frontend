/** @jest-environment jsdom */
import {
	codeLanguage,
	copyBlockText,
	copyClick,
	decorateCopyable,
	renderWithCopy,
	wrapsLines,
} from '../copy';

const parse = (html: string): HTMLElement => {
	const root = document.createElement('div');
	root.innerHTML = html;
	return root;
};

describe('wrapsLines', () => {
	it('wraps plaintext, text, txt and unlabelled code', () => {
		expect(wrapsLines('')).toBe(true);
		expect(wrapsLines('language-text')).toBe(true);
		expect(wrapsLines('language-plaintext')).toBe(true);
		expect(wrapsLines('language-TXT')).toBe(true);
	});

	it('keeps scrolling for real languages', () => {
		expect(wrapsLines('language-ts')).toBe(false);
		expect(wrapsLines('language-bash')).toBe(false);
		expect(codeLanguage('language-ts')).toBe('ts');
		expect(codeLanguage('')).toBeNull();
	});
});

describe('decorateCopyable', () => {
	it('puts a labelled copy button after every code block and blockquote', () => {
		const root = parse(
			decorateCopyable(
				'<pre><code>a</code></pre><blockquote><p>q</p></blockquote>',
			),
		);
		const buttons = root.querySelectorAll('button[data-md-copy]');
		expect(buttons).toHaveLength(2);
		buttons.forEach((b) => {
			expect(b.getAttribute('aria-label')).toBe('Copy');
			expect(b.getAttribute('title')).toBe('Copy');
			expect(b.nextElementSibling?.tagName).toMatch(/PRE|BLOCKQUOTE/);
		});
	});

	it('marks wrapping code and leaves real languages scrollable', () => {
		const root = parse(
			decorateCopyable(
				'<pre><code>a</code></pre><pre><code class="language-ts">b</code></pre>',
			),
		);
		const pres = root.querySelectorAll('pre');
		expect(pres[0].classList.contains('md-wrap')).toBe(true);
		expect(pres[1].classList.contains('md-wrap')).toBe(false);
	});

	it('gives a collapsed long block one button that copies the full code', () => {
		const lines = Array.from({ length: 60 }, (_, i) => `line ${i}`);
		const root = parse(renderWithCopy('```\n' + lines.join('\n') + '\n```'));
		const buttons = root.querySelectorAll('button[data-md-copy]');
		expect(buttons).toHaveLength(1);
		const block = buttons[0].parentElement as Element;
		expect(copyBlockText(block)).toBe(lines.join('\n'));
	});
});

describe('blockText', () => {
	it('returns code without fences or a trailing newline', () => {
		const root = parse(renderWithCopy('```text\nhello\nworld\n```'));
		const pre = root.querySelector('.md-copyable') as Element;
		expect(copyBlockText(pre)).toBe('hello\nworld');
	});

	it('returns a quote as plain paragraphs without markers', () => {
		const root = parse(renderWithCopy('> first line\n>\n> second **bold**'));
		const quote = root.querySelector('.md-copyable') as Element;
		expect(copyBlockText(quote)).toBe('first line\n\nsecond bold');
	});

	it('does not include the button of a nested block', () => {
		const root = parse(renderWithCopy('> quoted\n>\n> ```\n> code\n> ```'));
		const quote = root.querySelector('.md-copyable') as Element;
		expect(copyBlockText(quote)).toBe('quoted\n\ncode');
	});
});

describe('copyClick', () => {
	it('copies the block of the clicked button and reports it handled', async () => {
		const writeText = jest.fn().mockResolvedValue(undefined);
		Object.assign(navigator, { clipboard: { writeText } });
		const root = parse(renderWithCopy('```\nhi\n```'));
		document.body.appendChild(root);
		const button = root.querySelector('button[data-md-copy]') as HTMLElement;
		const event = new MouseEvent('click', { bubbles: true, cancelable: true });
		Object.defineProperty(event, 'target', { value: button });
		expect(copyClick(event)).toBe(true);
		await Promise.resolve();
		expect(writeText).toHaveBeenCalledWith('hi');
		expect(copyClick(new MouseEvent('click'))).toBe(false);
	});
});
