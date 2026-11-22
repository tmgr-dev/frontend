/** @jest-environment jsdom */
import { renderMarkdown } from '@/decisions/markdown';
import {
	addCopyButtons,
	copyBlockText,
	copyText,
	flashCopied,
	formatCtx,
	formatTime,
	groupMessages,
	quotePreview,
	renderBody,
	shownKind,
	statusTone,
} from '../format';
import type { ChatMessage } from '../types';

const msg = (
	id: string,
	ts: number,
	author = 'u1',
	kind: ChatMessage['kind'] = 'text',
): ChatMessage => ({
	v: 1,
	id,
	room: { kind: 'manager', machine: 'm' },
	author: { kind: 'owner', id: author },
	ts_ms: ts,
	body_md: id,
	kind,
});

describe('formatCtx', () => {
	it('formats tokens as k', () => {
		expect(formatCtx(142000)).toBe('142k');
		expect(formatCtx(142400)).toBe('142k');
		expect(formatCtx(950)).toBe('950');
		expect(formatCtx(0)).toBe('0');
	});
});

describe('formatTime', () => {
	const now = new Date(2026, 10, 21, 15, 0).getTime();
	it('shows HH:MM for today', () => {
		expect(formatTime(new Date(2026, 10, 21, 9, 5).getTime(), now)).toBe(
			'09:05',
		);
	});
	it('shows a date for other days', () => {
		expect(formatTime(new Date(2026, 10, 20, 23, 59).getTime(), now)).toBe(
			'Nov 20',
		);
	});
	it('adds the year for other years', () => {
		expect(formatTime(new Date(2025, 11, 31, 10, 0).getTime(), now)).toBe(
			'Dec 31, 2025',
		);
	});
});

describe('statusTone', () => {
	it('maps known statuses and falls back to idle', () => {
		expect(statusTone('working')).toBe('working');
		expect(statusTone('waiting')).toBe('waiting');
		expect(statusTone('failed')).toBe('failed');
		expect(statusTone('done')).toBe('done');
		expect(statusTone('idle')).toBe('idle');
		expect(statusTone(undefined)).toBe('idle');
		expect(statusTone('weird' as never)).toBe('idle');
	});
});

describe('groupMessages', () => {
	const d = (day: number, h: number) => new Date(2026, 10, day, h).getTime();

	it('inserts day separators', () => {
		const items = groupMessages([msg('a', d(20, 10)), msg('b', d(21, 10))]);
		expect(items.map((i) => i.type)).toEqual([
			'day',
			'message',
			'day',
			'message',
		]);
	});

	it('groups consecutive messages of one author on the same day', () => {
		const items = groupMessages([
			msg('a', d(21, 10)),
			msg('b', d(21, 11)),
			msg('c', d(21, 12), 'u2'),
			msg('d', d(21, 13)),
		]);
		const flags = items.flatMap((i) =>
			i.type === 'message' ? [i.grouped] : [],
		);
		expect(flags).toEqual([false, true, false, false]);
	});

	it('treats unknown kinds as system and never groups them', () => {
		expect(shownKind(msg('a', 1, 'u1', 'future'))).toBe('system');
		expect(shownKind(msg('a', 1, 'u1', 'decision'))).toBe('decision');
		const items = groupMessages([
			msg('a', d(21, 1), 'u1', 'future'),
			msg('b', d(21, 2), 'u1', 'future'),
		]);
		expect(
			items.flatMap((i) => (i.type === 'message' ? [i.grouped] : [])),
		).toEqual([false, false]);
	});

	it('does not group across days or non-text kinds', () => {
		const items = groupMessages([
			msg('a', d(20, 23)),
			msg('b', d(21, 1)),
			msg('c', d(21, 2), 'u1', 'system'),
			msg('e', d(21, 3), 'u1', 'system'),
		]);
		const flags = items.flatMap((i) =>
			i.type === 'message' ? [i.grouped] : [],
		);
		expect(flags).toEqual([false, false, false, false]);
	});

	it('returns an empty list for no messages', () => {
		expect(groupMessages([])).toEqual([]);
	});
});

describe('quotePreview', () => {
	it('strips markdown', () => {
		expect(
			quotePreview(
				'# Title\n\n**bold** _it_ `code` [link](http://x.y) ![img](a.png)',
			),
		).toBe('Title bold it code link img');
	});

	it('drops list and quote markers and fences', () => {
		expect(quotePreview('> quoted\n- item\n```js\nlet a\n```')).toBe(
			'quoted item js let a',
		);
	});

	it('truncates with an ellipsis', () => {
		const out = quotePreview('x'.repeat(300), 120);
		expect(out).toHaveLength(120);
		expect(out.endsWith('…')).toBe(true);
	});

	it('keeps short text intact', () => {
		expect(quotePreview('hello')).toBe('hello');
	});
});

describe('renderBody', () => {
	const parse = (html: string): HTMLElement => {
		const root = document.createElement('div');
		root.innerHTML = html;
		return root;
	};

	it('renders markdown', () => {
		expect(renderBody('**hi**')).toContain('<strong>hi</strong>');
		expect(renderBody('')).toBe('');
	});

	it('neutralises script tags', () => {
		const root = parse(renderBody('<script>alert(1)</script>\n\nok'));
		expect(root.querySelector('script')).toBeNull();
	});

	it('neutralises javascript: links', () => {
		const root = parse(renderBody('[x](javascript:alert(1))'));
		const href = root.querySelector('a')?.getAttribute('href') ?? '';
		expect(href.toLowerCase()).not.toContain('javascript:');
	});

	it('neutralises onerror handlers', () => {
		const root = parse(renderBody('<img src=x onerror="alert(1)">'));
		expect(root.querySelector('[onerror]')).toBeNull();
		expect(root.querySelector('img')).toBeNull();
	});
});

const copyLabels = { copy: 'Copy', copied: 'Copied' };
const withButtons = (md: string): HTMLElement => {
	const root = document.createElement('div');
	root.innerHTML = addCopyButtons(renderBody(md), copyLabels);
	return root;
};
const longCode = Array.from({ length: 60 }, (_, i) => `line ${i + 1}`);

describe('addCopyButtons', () => {
	it('puts one copy button next to a fenced code block', () => {
		const root = withButtons('```\nconst a = 1;\nreturn a;\n```');
		const buttons = root.querySelectorAll('button.blk-copy');
		expect(buttons).toHaveLength(1);
		expect(buttons[0].getAttribute('aria-label')).toBe('Copy');
		expect(buttons[0].getAttribute('type')).toBe('button');
		expect(buttons[0].nextElementSibling?.tagName).toBe('PRE');
	});

	it('copies a fenced code block without the fence', () => {
		const root = withButtons('```\nconst a = 1;\nreturn a;\n```');
		const block = root.querySelector('button.blk-copy')!.parentElement!;
		expect(copyBlockText(block)).toBe('const a = 1;\nreturn a;');
	});

	it('copies the full text of a code block longer than the preview', () => {
		const root = withButtons('```\n' + longCode.join('\n') + '\n```');
		expect(root.querySelectorAll('button.blk-copy')).toHaveLength(1);
		const block = root.querySelector('button.blk-copy')!.parentElement!;
		expect(copyBlockText(block)).toBe(longCode.join('\n'));
	});

	it('copies a quote without the > markers and keeps paragraph breaks as newlines', () => {
		const root = withButtons(
			'> first line\n> still first\n>\n> second\n>\n> > nested',
		);
		const block = root.querySelector('button.blk-copy')!.parentElement!;
		expect(copyBlockText(block)).toBe(
			'first line\nstill first\n\nsecond\n\nnested',
		);
	});

	it('copies the heading and body of a quote', () => {
		const root = withButtons('> ## Title\n> body');
		const block = root.querySelector('button.blk-copy')!.parentElement!;
		expect(copyBlockText(block)).toBe('Title\n\nbody');
	});

	it('copies the text of a list item and its nested list', () => {
		const root = withButtons('> - a\n>   - b');
		const block = root.querySelector('button.blk-copy')!.parentElement!;
		expect(copyBlockText(block)).toBe('a\nb');
	});

	it('copies the code text of a fenced block inside a quote', () => {
		const root = withButtons('> intro\n>\n> ```\n> const a = 1;\n> ```');
		const block = root.querySelector('button.blk-copy')!.parentElement!;
		expect(copyBlockText(block)).toBe('intro\n\nconst a = 1;');
	});

	it('gives a quote containing code exactly one button', () => {
		const root = withButtons('> intro\n>\n> ```\n> x\n> ```');
		expect(root.querySelectorAll('.blk-copy')).toHaveLength(1);
	});

	it('gives a button only to the outer of nested quotes', () => {
		const root = withButtons('> outer\n>\n> > inner');
		expect(root.querySelectorAll('button.blk-copy')).toHaveLength(1);
	});

	it('leaves inline code and plain text without buttons', () => {
		expect(
			withButtons('plain `x` text').querySelectorAll('button'),
		).toHaveLength(0);
	});

	it('keeps the decisions renderer free of copy buttons', () => {
		const html = renderMarkdown('```\nx\n```\n\n> quote');
		expect(html).toContain('<pre>');
		expect(html).not.toContain('blk-copy');
		expect(html).not.toContain('<button');
	});
});

describe('copyText', () => {
	const setClipboard = (value: unknown) =>
		Object.defineProperty(navigator, 'clipboard', {
			value,
			configurable: true,
		});
	const setExecCommand = (fn: jest.Mock) =>
		Object.defineProperty(document, 'execCommand', {
			value: fn,
			configurable: true,
		});

	afterEach(() => {
		setClipboard(undefined);
		setExecCommand(jest.fn(() => false));
	});

	it('uses the async clipboard API when it is available', async () => {
		const writeText = jest.fn().mockResolvedValue(undefined);
		setClipboard({ writeText });
		await expect(copyText('hello')).resolves.toBe(true);
		expect(writeText).toHaveBeenCalledWith('hello');
	});

	it('falls back to execCommand through a temporary textarea without a clipboard API', async () => {
		setClipboard(undefined);
		let seen: string | undefined;
		setExecCommand(
			jest.fn(() => {
				seen = document.querySelector('textarea')?.value;
				return true;
			}),
		);
		await expect(copyText('fallback text')).resolves.toBe(true);
		expect(seen).toBe('fallback text');
		expect(document.execCommand).toHaveBeenCalledWith('copy');
		expect(document.querySelector('textarea')).toBeNull();
	});

	it('falls back when writeText rejects', async () => {
		setClipboard({
			writeText: jest.fn().mockRejectedValue(new Error('denied')),
		});
		setExecCommand(jest.fn(() => true));
		await expect(copyText('x')).resolves.toBe(true);
		expect(document.execCommand).toHaveBeenCalledWith('copy');
	});

	it('reports failure when the fallback copy is refused', async () => {
		setClipboard(undefined);
		setExecCommand(jest.fn(() => false));
		await expect(copyText('x')).resolves.toBe(false);
	});
});

describe('flashCopied', () => {
	beforeEach(() => jest.useFakeTimers());
	afterEach(() => jest.useRealTimers());

	it('shows Copied for 1.5 s, restarts on repeat, then resets to Copy', () => {
		const root = withButtons('```\nx\n```');
		document.body.appendChild(root);
		const btn = root.querySelector<HTMLButtonElement>('button.blk-copy')!;

		flashCopied(btn);
		expect(btn.textContent).toBe('Copied');
		expect(btn.getAttribute('aria-label')).toBe('Copied');

		jest.advanceTimersByTime(1000);
		flashCopied(btn);
		jest.advanceTimersByTime(1499);
		expect(btn.textContent).toBe('Copied');

		jest.advanceTimersByTime(1);
		expect(btn.textContent).toBe('Copy');
		expect(btn.getAttribute('aria-label')).toBe('Copy');
		root.remove();
	});
});
