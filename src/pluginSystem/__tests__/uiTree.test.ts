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

describe('API 1.3: stack layout fields', () => {
	it('keeps valid gap/align/justify/grow and omits invalid or absent ones', () => {
		expect(
			sanitizeTree({
				type: 'stack',
				direction: 'row',
				gap: 'lg',
				align: 'center',
				justify: 'between',
				grow: true,
				children: [],
			}),
		).toEqual({
			type: 'stack',
			direction: 'row',
			gap: 'lg',
			align: 'center',
			justify: 'between',
			grow: true,
			children: [],
		});
		expect(
			sanitizeTree({
				type: 'stack',
				gap: 'huge',
				align: 'middle',
				justify: 'around',
				grow: 'yes',
				children: [],
			}),
		).toEqual({ type: 'stack', direction: 'column', children: [] });
		expect(
			(sanitizeTree({ type: 'stack', grow: false, children: [] }) as any).grow,
		).toBeUndefined();
	});
});

describe('API 1.3: card', () => {
	it('defaults tone to default and padding to md, and drops invalid values', () => {
		expect(sanitizeTree({ type: 'card', children: [] })).toEqual({
			type: 'card',
			children: [],
			tone: 'default',
			padding: 'md',
		});
		expect(
			sanitizeTree({ type: 'card', tone: 'raised', padding: 'lg', children: [] }),
		).toEqual({ type: 'card', children: [], tone: 'raised', padding: 'lg' });
		expect(
			sanitizeTree({ type: 'card', tone: 'huge', padding: 'huge', children: [] }),
		).toEqual({ type: 'card', children: [], tone: 'default', padding: 'md' });
	});

	it('omits accent when absent, falls back to gray for any invalid value, and keeps a known color', () => {
		expect(
			(sanitizeTree({ type: 'card', children: [] }) as any).accent,
		).toBeUndefined();
		expect(
			(sanitizeTree({ type: 'card', accent: 'pink', children: [] }) as any).accent,
		).toBe('gray');
		expect(
			(sanitizeTree({ type: 'card', accent: null, children: [] }) as any).accent,
		).toBe('gray');
		expect(
			(sanitizeTree({ type: 'card', accent: 'purple', children: [] }) as any).accent,
		).toBe('purple');
	});

	it('keeps grow only when exactly true', () => {
		expect((sanitizeTree({ type: 'card', grow: true, children: [] }) as any).grow).toBe(
			true,
		);
		expect(
			(sanitizeTree({ type: 'card', grow: 'yes', children: [] }) as any).grow,
		).toBeUndefined();
	});

	it('validates onClick like a button, dropping it (but keeping the card) when invalid', () => {
		expect(
			sanitizeTree({
				type: 'card',
				onClick: { command: 'a.b', args: { x: 1 } },
				children: [{ type: 'heading', text: 'Group A', level: 2 }],
			}),
		).toEqual({
			type: 'card',
			children: [{ type: 'heading', text: 'Group A', level: 2 }],
			tone: 'default',
			padding: 'md',
			onClick: { command: 'a.b', args: { x: 1 } },
			label: 'Group A',
		});
		const noCommand = sanitizeTree({
			type: 'card',
			onClick: {},
			children: [{ type: 'text', text: 'x' }],
		}) as any;
		expect(noCommand.onClick).toBeUndefined();
		expect(noCommand.label).toBeUndefined();
		expect(noCommand.type).toBe('card');
	});

	it('takes the label from the first non-empty heading/text/badge/stat found depth-first, or falls back to Card', () => {
		expect(
			(
				sanitizeTree({
					type: 'card',
					onClick: { command: 'a.b' },
					children: [
						{
							type: 'stack',
							children: [{ type: 'badge', text: 'Overrun', color: 'red' }],
						},
						{ type: 'stat', label: 'Spent', value: '3h' },
					],
				}) as any
			).label,
		).toBe('Overrun');
		expect(
			(
				sanitizeTree({
					type: 'card',
					onClick: { command: 'a.b' },
					children: [{ type: 'divider' }],
				}) as any
			).label,
		).toBe('Card');
	});

	it('demotes a card nested past 3 ancestor cards into a stack, keeping its content', () => {
		const deep = {
			type: 'card',
			children: [
				{
					type: 'card',
					children: [
						{
							type: 'card',
							children: [
								{
									type: 'card',
									children: [{ type: 'text', text: 'bottom' }],
								},
							],
						},
					],
				},
			],
		};
		const outer = sanitizeTree(deep) as any;
		expect(outer.type).toBe('card');
		const level2 = outer.children[0];
		expect(level2.type).toBe('card');
		const level3 = level2.children[0];
		expect(level3.type).toBe('card');
		const level4 = level3.children[0];
		expect(level4).toEqual({
			type: 'stack',
			direction: 'column',
			children: [{ type: 'text', text: 'bottom', tone: 'default' }],
		});
	});
});

describe('API 1.3: grid', () => {
	it('clamps columns to 1-6 and rounds, defaulting a non-number to 1', () => {
		expect((sanitizeTree({ type: 'grid', columns: 4, children: [] }) as any).columns).toBe(4);
		expect((sanitizeTree({ type: 'grid', columns: 0, children: [] }) as any).columns).toBe(1);
		expect((sanitizeTree({ type: 'grid', columns: 12, children: [] }) as any).columns).toBe(6);
		expect((sanitizeTree({ type: 'grid', columns: 2.6, children: [] }) as any).columns).toBe(3);
		expect(
			(sanitizeTree({ type: 'grid', columns: 'lots', children: [] }) as any).columns,
		).toBe(1);
	});

	it('clamps minWidth to 160-480, defaulting a non-number to 200, and defaults gap to md', () => {
		expect(
			(sanitizeTree({ type: 'grid', columns: 2, minWidth: 100, children: [] }) as any)
				.minWidth,
		).toBe(160);
		expect(
			(sanitizeTree({ type: 'grid', columns: 2, minWidth: 900, children: [] }) as any)
				.minWidth,
		).toBe(480);
		expect(
			(sanitizeTree({ type: 'grid', columns: 2, children: [] }) as any).minWidth,
		).toBe(200);
		expect((sanitizeTree({ type: 'grid', columns: 2, children: [] }) as any).gap).toBe(
			'md',
		);
		expect(
			(sanitizeTree({ type: 'grid', columns: 2, gap: 'sm', children: [] }) as any).gap,
		).toBe('sm');
	});
});

describe('API 1.3: menu', () => {
	it('validates items like buttons, keeps at most 20, and returns null once nothing survives', () => {
		expect(
			sanitizeTree({
				type: 'menu',
				items: [
					{ text: 'Archive', command: 'a.archive' },
					{ text: 'Delete', command: 'a.delete', confirm: 'Sure?' },
					{ text: 'Bad', command: '' },
				],
			}),
		).toEqual({
			type: 'menu',
			icon: 'more',
			items: [
				{ text: 'Archive', command: 'a.archive' },
				{ text: 'Delete', command: 'a.delete', confirm: 'Sure?' },
			],
		});
		expect(
			sanitizeTree({ type: 'menu', items: [{ text: 'Bad', command: '' }] }),
		).toBeNull();
		expect(sanitizeTree({ type: 'menu', items: [] })).toBeNull();
		const many = sanitizeTree({
			type: 'menu',
			items: Array.from({ length: 30 }, (_, i) => ({ text: `i${i}`, command: `c.${i}` })),
		}) as any;
		expect(many.items).toHaveLength(20);
	});

	it('keeps an optional label capped at 60 chars', () => {
		expect(
			(
				sanitizeTree({
					type: 'menu',
					label: 'Row actions',
					items: [{ text: 'Go', command: 'a.b' }],
				}) as any
			).label,
		).toBe('Row actions');
		expect(
			(
				sanitizeTree({
					type: 'menu',
					label: 'x'.repeat(90),
					items: [{ text: 'Go', command: 'a.b' }],
				}) as any
			).label,
		).toHaveLength(60);
	});
});

describe('API 1.3: button variant/size, stat/badge command', () => {
	it('keeps a valid button variant and size, and omits them when invalid or absent', () => {
		expect(
			sanitizeTree({
				type: 'button',
				text: 'Go',
				command: 'a.b',
				variant: 'primary',
				size: 'sm',
			}),
		).toEqual({ type: 'button', text: 'Go', command: 'a.b', variant: 'primary', size: 'sm' });
		const bad = sanitizeTree({
			type: 'button',
			text: 'Go',
			command: 'a.b',
			variant: 'huge',
			size: 'xl',
		}) as any;
		expect(bad.variant).toBeUndefined();
		expect(bad.size).toBeUndefined();
	});

	it('keeps a valid command (+args) on stat and badge, and drops it when invalid, without dropping the node', () => {
		expect(
			sanitizeTree({
				type: 'stat',
				label: 'Overrun',
				value: '120%',
				command: 'a.open',
				args: { id: 1 },
			}),
		).toEqual({
			type: 'stat',
			label: 'Overrun',
			value: '120%',
			tone: 'default',
			command: 'a.open',
			args: { id: 1 },
		});
		const badStat = sanitizeTree({ type: 'stat', label: 'x', value: 'y', command: 1 }) as any;
		expect(badStat.command).toBeUndefined();
		expect(badStat.type).toBe('stat');

		expect(
			sanitizeTree({ type: 'badge', text: 'A', color: 'blue', command: 'a.open' }),
		).toEqual({ type: 'badge', text: 'A', color: 'blue', command: 'a.open' });
		const badBadge = sanitizeTree({ type: 'badge', text: 'A', command: 1 }) as any;
		expect(badBadge.command).toBeUndefined();
		expect(badBadge.type).toBe('badge');
	});
});

describe('an older apiMinor downgrades API 1.3 nodes and drops the new optional fields', () => {
	it('downgrades card to a column stack and grid to a row stack, recursively', () => {
		expect(
			sanitizeTree(
				{
					type: 'card',
					tone: 'raised',
					accent: 'red',
					onClick: { command: 'a.b' },
					children: [{ type: 'text', text: 'x' }],
				},
				undefined,
				2,
			),
		).toEqual({
			type: 'stack',
			direction: 'column',
			children: [{ type: 'text', text: 'x', tone: 'default' }],
		});
		expect(
			sanitizeTree(
				{ type: 'grid', columns: 3, children: [{ type: 'text', text: 'x' }] },
				undefined,
				2,
			),
		).toEqual({
			type: 'stack',
			direction: 'row',
			children: [{ type: 'text', text: 'x', tone: 'default' }],
		});
	});

	it('downgrades menu to an empty column stack, discarding its items', () => {
		expect(
			sanitizeTree(
				{ type: 'menu', items: [{ text: 'Go', command: 'a.b' }] },
				undefined,
				2,
			),
		).toEqual({ type: 'stack', direction: 'column', children: [] });
	});

	it('drops the new optional fields on stack/button/stat/badge, matching today\'s output exactly', () => {
		expect(
			sanitizeTree(
				{
					type: 'stack',
					direction: 'row',
					gap: 'lg',
					align: 'center',
					justify: 'between',
					grow: true,
					children: [],
				},
				undefined,
				2,
			),
		).toStrictEqual({ type: 'stack', direction: 'row', children: [] });
		expect(
			sanitizeTree(
				{ type: 'button', text: 'Go', command: 'a.b', variant: 'primary', size: 'sm' },
				undefined,
				2,
			),
		).toStrictEqual({ type: 'button', text: 'Go', command: 'a.b' });
		expect(
			sanitizeTree(
				{ type: 'stat', label: 'x', value: 'y', command: 'a.b' },
				undefined,
				2,
			),
		).toStrictEqual({ type: 'stat', label: 'x', value: 'y', tone: 'default' });
		expect(
			sanitizeTree(
				{ type: 'badge', text: 'x', color: 'red', command: 'a.b' },
				undefined,
				2,
			),
		).toStrictEqual({ type: 'badge', text: 'x', color: 'red' });
	});

	it('leaves nodes that existed before API 1.3 completely unchanged', () => {
		const tree = {
			type: 'stack',
			children: [
				{ type: 'heading', text: 'Title', level: 1 },
				{ type: 'taskLink', taskId: 3, text: 'TM-3' },
			],
		};
		expect(sanitizeTree(tree, undefined, 2)).toEqual(sanitizeTree(tree, undefined, 3));
	});
});
