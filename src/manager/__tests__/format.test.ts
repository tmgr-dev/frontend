/** @jest-environment jsdom */
import {
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
