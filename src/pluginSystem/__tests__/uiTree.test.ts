import { isLinkAllowed, sanitizeTree, type LinkContext } from '../uiTree';

const ALLOWED: LinkContext = { allowedDomains: ['a.example.com'], linksOpen: true };

it('keeps known components and normalises their props', () => {
	expect(
		sanitizeTree({
			type: 'stack',
			children: [
				{ type: 'heading', text: 'Overrun', level: 9 },
				{ type: 'text', text: 'Two tasks', tone: 'loud' },
				{ type: 'badge', text: '120%', color: 'red' },
				{ type: 'progress', value: 1.4, color: 'red' },
				{
					type: 'button',
					text: 'Refresh',
					command: 'tmgr.e.refresh',
					args: { force: true },
				},
				{ type: 'taskLink', taskId: 7, text: 'TM-7' },
				{ type: 'divider' },
			],
		}),
	).toEqual({
		type: 'stack',
		direction: 'column',
		children: [
			{ type: 'heading', text: 'Overrun', level: 2 },
			{ type: 'text', text: 'Two tasks', tone: 'default' },
			{ type: 'badge', text: '120%', color: 'red' },
			{ type: 'progress', value: 1.4, color: 'red' },
			{
				type: 'button',
				text: 'Refresh',
				command: 'tmgr.e.refresh',
				args: { force: true },
			},
			{ type: 'taskLink', taskId: 7, text: 'TM-7' },
			{ type: 'divider' },
		],
	});
});

it('drops unknown components and anything that is not plain data', () => {
	expect(
		sanitizeTree({
			type: 'stack',
			children: [
				{ type: 'html', html: '<img onerror=alert(1)>' },
				{ type: 'text', text: { toString: 1 } },
				'plain',
			],
		}),
	).toEqual({
		type: 'stack',
		direction: 'column',
		children: [{ type: 'text', text: 'plain', tone: 'default' }],
	});
	expect(sanitizeTree(null)).toBeNull();
});

it('builds tables from declared columns only', () => {
	expect(
		sanitizeTree({
			type: 'table',
			columns: [
				{ key: 'task', title: 'Task' },
				{ key: 'ratio', title: 'Spent' },
			],
			rows: [
				{
					taskId: 3,
					cells: {
						task: { type: 'taskLink', taskId: 3, text: 'A' },
						ratio: 1.2,
						extra: 'x',
					},
				},
			],
		}),
	).toEqual({
		type: 'table',
		columns: [
			{ key: 'task', title: 'Task' },
			{ key: 'ratio', title: 'Spent' },
		],
		rows: [
			{
				taskId: 3,
				cells: {
					task: { type: 'taskLink', taskId: 3, text: 'A' },
					ratio: { type: 'text', text: '1.2', tone: 'default' },
				},
			},
		],
	});
});

it('caps size and depth', () => {
	let deep: any = { type: 'text', text: 'bottom' };
	for (let i = 0; i < 20; i++) deep = { type: 'stack', children: [deep] };
	expect(JSON.stringify(sanitizeTree(deep))).not.toContain('bottom');
	const wide = sanitizeTree({
		type: 'list',
		items: Array.from({ length: 5000 }, (_, i) => `item ${i}`),
	}) as any;
	expect(wide.items.length).toBeLessThanOrEqual(500);
	expect(
		(sanitizeTree({ type: 'text', text: 'x'.repeat(5000) }) as any).text.length,
	).toBe(1000);
});

it('accepts the purple and orange badge colors and falls back on anything else', () => {
	expect(sanitizeTree({ type: 'badge', text: 'A', color: 'purple' })).toEqual({
		type: 'badge',
		text: 'A',
		color: 'purple',
	});
	expect(sanitizeTree({ type: 'badge', text: 'B', color: 'orange' })).toEqual({
		type: 'badge',
		text: 'B',
		color: 'orange',
	});
	expect(sanitizeTree({ type: 'badge', text: 'C', color: 'pink' })).toEqual({
		type: 'badge',
		text: 'C',
		color: 'gray',
	});
});

it('renders badges found inside a list like any other item', () => {
	expect(
		sanitizeTree({
			type: 'list',
			items: [
				{ type: 'badge', text: 'One', color: 'green' },
				{ type: 'badge', text: 'Two', color: 'red' },
			],
		}),
	).toEqual({
		type: 'list',
		items: [
			{ type: 'badge', text: 'One', color: 'green' },
			{ type: 'badge', text: 'Two', color: 'red' },
		],
	});
});

it('keeps a confirm prompt on a button, capped, and drops it when absent', () => {
	expect(
		sanitizeTree({
			type: 'button',
			text: 'Delete',
			command: 'a.b',
			confirm: 'x'.repeat(300),
		}),
	).toEqual({
		type: 'button',
		text: 'Delete',
		command: 'a.b',
		confirm: 'x'.repeat(200),
	});
	expect(
		(sanitizeTree({ type: 'button', text: 'Go', command: 'a.b' }) as any)
			.confirm,
	).toBeUndefined();
	expect(
		(
			sanitizeTree({
				type: 'button',
				text: 'Go',
				command: 'a.b',
				confirm: '   ',
			}) as any
		).confirm,
	).toBeUndefined();
});

it('drops a button whose args are over 8 KB', () => {
	expect(
		sanitizeTree({
			type: 'button',
			text: 'Go',
			command: 'a.b',
			args: { big: 'x'.repeat(8 * 1024) },
		}),
	).toBeNull();
	expect(
		sanitizeTree({
			type: 'button',
			text: 'Go',
			command: 'a.b',
			args: { ok: 'x'.repeat(100) },
		}),
	).toEqual({
		type: 'button',
		text: 'Go',
		command: 'a.b',
		args: { ok: 'x'.repeat(100) },
	});
});

it('caps a copyable node and keeps its optional label', () => {
	expect(
		sanitizeTree({ type: 'copyable', text: 'id-123', label: 'Task id' }),
	).toEqual({ type: 'copyable', text: 'id-123', label: 'Task id' });
	expect(
		(sanitizeTree({ type: 'copyable', text: 'x'.repeat(3000) }) as any).text
			.length,
	).toBe(2000);
	expect(sanitizeTree({ type: 'copyable', text: 42 })).toBeNull();
});

it('drops timeAgo and dueTime with an unparsable date, keeps a valid one', () => {
	expect(
		sanitizeTree({ type: 'timeAgo', at: '2026-09-27T10:00:00Z' }),
	).toEqual({ type: 'timeAgo', at: '2026-09-27T10:00:00Z' });
	expect(
		sanitizeTree({ type: 'dueTime', at: '2026-09-27T10:00:00Z' }),
	).toEqual({ type: 'dueTime', at: '2026-09-27T10:00:00Z' });
	expect(sanitizeTree({ type: 'timeAgo', at: 'not a date' })).toBeNull();
	expect(sanitizeTree({ type: 'dueTime', at: 123 })).toBeNull();
});

it('drops timeAgo and dueTime whose at string is over 64 chars, even if parsable', () => {
	const long = 'Sun Sep 27 2026 10:00:00 GMT+0000 (Coordinated Universal Time Zone)';
	expect(long.length).toBeGreaterThan(64);
	expect(Number.isNaN(Date.parse(long))).toBe(false);
	expect(sanitizeTree({ type: 'timeAgo', at: long })).toBeNull();
	expect(sanitizeTree({ type: 'dueTime', at: long })).toBeNull();
});

it('builds a keyValue list from valid rows only, capped at 50', () => {
	expect(
		sanitizeTree({
			type: 'keyValue',
			items: [
				{ key: 'Owner', value: 'Ann' },
				{ key: 'Status', value: { type: 'badge', text: 'Open', color: 'blue' } },
				{ key: 'x'.repeat(90), value: undefined },
			],
		}),
	).toEqual({
		type: 'keyValue',
		items: [
			{ key: 'Owner', value: { type: 'text', text: 'Ann', tone: 'default' } },
			{ key: 'Status', value: { type: 'badge', text: 'Open', color: 'blue' } },
		],
	});
	const many = sanitizeTree({
		type: 'keyValue',
		items: Array.from({ length: 80 }, (_, i) => ({ key: `k${i}`, value: 'v' })),
	}) as any;
	expect(many.items.length).toBe(50);
});

describe('link nodes', () => {
	it('keeps an https link on an allowed domain with links:open, and shows its host', () => {
		expect(
			sanitizeTree(
				{ type: 'link', url: 'https://a.example.com/x', text: 'Open it' },
				ALLOWED,
			),
		).toEqual({
			type: 'link',
			url: 'https://a.example.com/x',
			text: 'Open it',
			host: 'a.example.com',
		});
	});

	it('falls back to plain text for http, an unlisted domain, or a plugin without links:open', () => {
		const asText = (url: string, context: LinkContext) =>
			sanitizeTree({ type: 'link', url, text: 'Open it' }, context);
		expect(asText('http://a.example.com/x', ALLOWED)).toEqual({
			type: 'text',
			text: 'Open it',
			tone: 'default',
		});
		expect(asText('https://evil.example.com/x', ALLOWED)).toEqual({
			type: 'text',
			text: 'Open it',
			tone: 'default',
		});
		expect(
			asText('https://a.example.com/x', {
				allowedDomains: ['a.example.com'],
				linksOpen: false,
			}),
		).toEqual({ type: 'text', text: 'Open it', tone: 'default' });
	});

	it('exposes the same check as isLinkAllowed, for the host to reuse at click time', () => {
		expect(isLinkAllowed('https://a.example.com/x', ALLOWED)).toBe(true);
		expect(isLinkAllowed('https://sub.a.example.com/x', ALLOWED)).toBe(false);
		expect(isLinkAllowed('not a url', ALLOWED)).toBe(false);
	});
});

it('counts table rows against the node budget, so nested tables cannot multiply', () => {
	const row = { cells: { a: 'x' } };
	const inner = {
		type: 'table',
		columns: [{ key: 'a', title: 'A' }],
		rows: Array.from({ length: 500 }, () => row),
	};
	const outer = {
		type: 'table',
		columns: [{ key: 'a', title: 'A' }],
		rows: Array.from({ length: 500 }, () => ({ cells: { a: inner } })),
	};
	const count = (node: any): number =>
		node.type === 'table'
			? node.rows.length +
			  node.rows.reduce(
					(sum: number, r: any) =>
						sum +
						(Object.values(r.cells) as any[]).reduce(
							(s: number, c: any) => s + count(c),
							0,
						),
					0,
			  )
			: 1;
	expect(count(sanitizeTree(outer))).toBeLessThanOrEqual(3000);
});
