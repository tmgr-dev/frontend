import {
	calendarDate,
	cutCodePoints,
	ICON_NAMES,
	isLinkAllowed,
	MAX_TEXTAREA_VALUE,
	NO_LINKS,
	sanitizeTree,
	type LinkContext,
} from '../uiTree';

const ALLOWED: LinkContext = {
	allowedDomains: ['a.example.com'],
	linksOpen: true,
};

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
		(
			sanitizeTree(
				{ type: 'text', text: 'x'.repeat(5000) },
				undefined,
				6,
			) as any
		).text.length,
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
	expect(sanitizeTree({ type: 'timeAgo', at: '2026-09-27T10:00:00Z' })).toEqual(
		{ type: 'timeAgo', at: '2026-09-27T10:00:00Z' },
	);
	expect(sanitizeTree({ type: 'dueTime', at: '2026-09-27T10:00:00Z' })).toEqual(
		{ type: 'dueTime', at: '2026-09-27T10:00:00Z' },
	);
	expect(sanitizeTree({ type: 'timeAgo', at: 'not a date' })).toBeNull();
	expect(sanitizeTree({ type: 'dueTime', at: 123 })).toBeNull();
});

it('drops timeAgo and dueTime whose at string is over 64 chars, even if parsable', () => {
	const long =
		'Sun Sep 27 2026 10:00:00 GMT+0000 (Coordinated Universal Time Zone)';
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
				{
					key: 'Status',
					value: { type: 'badge', text: 'Open', color: 'blue' },
				},
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
			sanitizeTree({
				type: 'card',
				tone: 'raised',
				padding: 'lg',
				children: [],
			}),
		).toEqual({ type: 'card', children: [], tone: 'raised', padding: 'lg' });
		expect(
			sanitizeTree({
				type: 'card',
				tone: 'huge',
				padding: 'huge',
				children: [],
			}),
		).toEqual({ type: 'card', children: [], tone: 'default', padding: 'md' });
	});

	it('omits accent when absent, falls back to gray for any invalid value, and keeps a known color', () => {
		expect(
			(sanitizeTree({ type: 'card', children: [] }) as any).accent,
		).toBeUndefined();
		expect(
			(sanitizeTree({ type: 'card', accent: 'pink', children: [] }) as any)
				.accent,
		).toBe('gray');
		expect(
			(sanitizeTree({ type: 'card', accent: null, children: [] }) as any)
				.accent,
		).toBe('gray');
		expect(
			(sanitizeTree({ type: 'card', accent: 'purple', children: [] }) as any)
				.accent,
		).toBe('purple');
	});

	it('keeps grow only when exactly true', () => {
		expect(
			(sanitizeTree({ type: 'card', grow: true, children: [] }) as any).grow,
		).toBe(true);
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
		expect(
			(sanitizeTree({ type: 'grid', columns: 4, children: [] }) as any).columns,
		).toBe(4);
		expect(
			(sanitizeTree({ type: 'grid', columns: 0, children: [] }) as any).columns,
		).toBe(1);
		expect(
			(sanitizeTree({ type: 'grid', columns: 12, children: [] }) as any)
				.columns,
		).toBe(6);
		expect(
			(sanitizeTree({ type: 'grid', columns: 2.6, children: [] }) as any)
				.columns,
		).toBe(3);
		expect(
			(sanitizeTree({ type: 'grid', columns: 'lots', children: [] }) as any)
				.columns,
		).toBe(1);
	});

	it('clamps minWidth to 160-480, defaulting a non-number to 200, and defaults gap to md', () => {
		expect(
			(
				sanitizeTree({
					type: 'grid',
					columns: 2,
					minWidth: 100,
					children: [],
				}) as any
			).minWidth,
		).toBe(160);
		expect(
			(
				sanitizeTree({
					type: 'grid',
					columns: 2,
					minWidth: 900,
					children: [],
				}) as any
			).minWidth,
		).toBe(480);
		expect(
			(sanitizeTree({ type: 'grid', columns: 2, children: [] }) as any)
				.minWidth,
		).toBe(200);
		expect(
			(sanitizeTree({ type: 'grid', columns: 2, children: [] }) as any).gap,
		).toBe('md');
		expect(
			(
				sanitizeTree({
					type: 'grid',
					columns: 2,
					gap: 'sm',
					children: [],
				}) as any
			).gap,
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
			items: Array.from({ length: 30 }, (_, i) => ({
				text: `i${i}`,
				command: `c.${i}`,
			})),
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
		).toEqual({
			type: 'button',
			text: 'Go',
			command: 'a.b',
			variant: 'primary',
			size: 'sm',
		});
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
		const badStat = sanitizeTree({
			type: 'stat',
			label: 'x',
			value: 'y',
			command: 1,
		}) as any;
		expect(badStat.command).toBeUndefined();
		expect(badStat.type).toBe('stat');

		expect(
			sanitizeTree({
				type: 'badge',
				text: 'A',
				color: 'blue',
				command: 'a.open',
			}),
		).toEqual({ type: 'badge', text: 'A', color: 'blue', command: 'a.open' });
		const badBadge = sanitizeTree({
			type: 'badge',
			text: 'A',
			command: 1,
		}) as any;
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

	it("drops the new optional fields on stack/button/stat/badge, matching today's output exactly", () => {
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
				{
					type: 'button',
					text: 'Go',
					command: 'a.b',
					variant: 'primary',
					size: 'sm',
				},
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
		expect(sanitizeTree(tree, undefined, 2)).toEqual(
			sanitizeTree(tree, undefined, 3),
		);
	});
});

describe('text and documents (API 1.7)', () => {
	const LINKS: LinkContext = {
		allowedDomains: ['allowed.example'],
		linksOpen: true,
	};
	const md = (text: string, minor = 7, context = LINKS) =>
		sanitizeTree({ type: 'markdown', text }, context, minor) as any;
	const collect = (value: unknown, type: string, found: any[] = []): any[] => {
		if (Array.isArray(value)) value.forEach((v) => collect(v, type, found));
		else if (value && typeof value === 'object') {
			if ((value as any).type === type) found.push(value);
			Object.values(value).forEach((v) => collect(v, type, found));
		}
		return found;
	};
	const firstInline = (tree: any) => tree.blocks[0].children;

	describe('version gating', () => {
		it('drops markdown and code below 1.7', () => {
			expect(md('# hi', 6)).toBeNull();
			expect(sanitizeTree({ type: 'code', text: 'x' }, LINKS, 6)).toBeNull();
		});

		it('ignores spans and keeps the 1000 cap below 1.7', () => {
			const tree = sanitizeTree(
				{ type: 'text', text: 'a'.repeat(5000), spans: [{ text: 'z' }] },
				LINKS,
				6,
			) as any;
			expect(tree.text).toHaveLength(1000);
			expect(tree.spans).toBeUndefined();
		});
	});

	describe('text spans', () => {
		it('joins span texts and keeps only true flags', () => {
			expect(
				sanitizeTree({
					type: 'text',
					text: 'ignored',
					spans: [
						{ text: 'Hello ', bold: true, italic: 'yes', unknown: true },
						{ text: 'world', code: true, strike: false },
						{ text: 42 },
						{ text: '' },
						null,
					],
				}),
			).toEqual({
				type: 'text',
				text: 'Hello world',
				tone: 'default',
				spans: [
					{ text: 'Hello ', bold: true },
					{ text: 'world', code: true },
				],
			});
		});

		it('caps the number of spans', () => {
			const tree = sanitizeTree({
				type: 'text',
				spans: Array.from({ length: 500 }, () => ({ text: 'a' })),
			}) as any;
			expect(tree.spans).toHaveLength(200);
			expect(tree.text).toHaveLength(200);
		});

		it('flags newlines in long text', () => {
			const tree = sanitizeTree({ type: 'text', text: 'a\nb' }) as any;
			expect(tree.breaks).toBe(true);
		});
	});

	describe('code node', () => {
		it('normalises lang and wrap and caps the text', () => {
			expect(
				sanitizeTree({
					type: 'code',
					text: 'x'.repeat(30000),
					lang: 'l'.repeat(50),
				}),
			).toEqual({
				type: 'code',
				text: 'x'.repeat(20000),
				lang: 'l'.repeat(30),
				wrap: true,
			});
			expect(
				(sanitizeTree({ type: 'code', text: 'a', wrap: false }) as any).wrap,
			).toBe(false);
			expect(sanitizeTree({ type: 'code', text: 5 })).toBeNull();
		});
	});

	describe('limits', () => {
		it('lets text nodes reach 20000 chars but keeps other strings at 1000', () => {
			expect(
				(sanitizeTree({ type: 'text', text: 'a'.repeat(30000) }) as any).text,
			).toHaveLength(20000);
			expect(
				(sanitizeTree({ type: 'heading', text: 'a'.repeat(30000) }) as any)
					.text,
			).toHaveLength(1000);
		});

		it('shares a long-text budget across nodes', () => {
			const tree = sanitizeTree({
				type: 'stack',
				children: Array.from({ length: 15 }, () => ({
					type: 'text',
					text: 'a'.repeat(20000),
				})),
			}) as any;
			const lengths = tree.children.map((c: any) => c.text.length);
			expect(lengths.slice(0, 10).every((n: number) => n === 20000)).toBe(true);
			expect(lengths[10]).toBe(11000);
			expect(lengths.slice(11).every((n: number) => n === 1000)).toBe(true);
		});

		it('never lets text past 1000 chars exceed the long-text budget in total', () => {
			const tree = sanitizeTree({
				type: 'stack',
				children: Array.from({ length: 40 }, (_, i) => ({
					type: i % 2 ? 'code' : 'text',
					text: 'a'.repeat(20000),
				})),
			}) as any;
			const extra = tree.children.reduce(
				(sum: number, c: any) => sum + Math.max(0, c.text.length - 1000),
				0,
			);
			expect(extra).toBe(200000);
		});

		it('charges spans against the node budget', () => {
			const spans = Array.from({ length: 200 }, () => ({ text: 'x' }));
			const tree = sanitizeTree({
				type: 'stack',
				children: Array.from({ length: 6 }, () => ({
					type: 'stack',
					children: Array.from({ length: 500 }, () => ({
						type: 'text',
						text: 'x',
						spans,
					})),
				})),
			}) as any;
			const count = collect(tree, 'text').reduce(
				(sum: number, t: any) => sum + 1 + (t.spans?.length ?? 0),
				0,
			);
			expect(count).toBeLessThanOrEqual(3000);
		});

		it('keeps inline lexing bounded on pathological emphasis', () => {
			const started = Date.now();
			const tree = sanitizeTree({
				type: 'stack',
				children: Array.from({ length: 12 }, () => ({
					type: 'markdown',
					text: '*a '.repeat(7000),
				})),
			}) as any;
			expect(Date.now() - started).toBeLessThan(3000);
			expect(tree.children).toHaveLength(12);
		});

		it('shows a very long paragraph as plain text', () => {
			const tree = md(`**bold** ${'word '.repeat(1000)}`);
			expect(collect(tree, 'strong')).toHaveLength(0);
			expect(tree.blocks[0].children[0].text.startsWith('**bold**')).toBe(true);
		});

		it('caps markdown source at 20000 chars', () => {
			const tree = md('a'.repeat(30000));
			expect(tree.blocks[0].children[0].text).toHaveLength(20000);
		});

		it('spends the node budget on a huge markdown document', () => {
			const tree = md('- a\n'.repeat(5000));
			const count = JSON.stringify(tree).match(/"type"/g)!.length;
			expect(count).toBeLessThanOrEqual(3000);
			expect(count).toBeGreaterThan(100);
		});

		it('does not throw on deeply nested quotes and lists', () => {
			for (const source of [
				'>'.repeat(5000),
				'- '.repeat(5000),
				'1. '.repeat(5000),
			]) {
				const tree = md(source);
				expect(tree).not.toBeNull();
				const depth = (value: any): number =>
					value && typeof value === 'object'
						? Math.max(0, ...Object.values(value).map(depth)) +
							(value.type === 'blockquote' || value.type === 'list' ? 1 : 0)
						: 0;
				expect(depth(tree)).toBeLessThanOrEqual(8);
			}
		});

		it('caps table columns and rows', () => {
			const cols = 20;
			const row = (cell: string) => `| ${Array(cols).fill(cell).join(' | ')} |`;
			const source = [
				row('h'),
				`| ${Array(cols).fill('---').join(' | ')} |`,
				...Array.from({ length: 300 }, () => row('c')),
			].join('\n');
			const table = md(source).blocks[0];
			expect(table.type).toBe('table');
			expect(table.header).toHaveLength(12);
			expect(table.align).toHaveLength(12);
			expect(table.rows).toHaveLength(200);
			expect(table.rows[0]).toHaveLength(12);
		});
	});

	describe('structure', () => {
		it('converts the whitelisted blocks', () => {
			const tree = md(
				[
					'# One',
					'#### Four',
					'',
					'Para **b** *i* ~~d~~ `c`',
					'',
					'- [x] done',
					'- [ ] todo',
					'',
					'3. third',
					'',
					'| a | b |',
					'|:--|--:|',
					'| 1 | 2 |',
					'',
					'```js title',
					'const a = 1;',
					'```',
					'',
					'> quote',
					'',
					'---',
				].join('\n'),
			);
			const types = tree.blocks.map((b: any) => b.type);
			expect(types).toEqual([
				'heading',
				'heading',
				'paragraph',
				'list',
				'list',
				'table',
				'code',
				'blockquote',
				'hr',
			]);
			expect(tree.blocks[1].depth).toBe(3);
			expect(tree.blocks[3].items.map((i: any) => i.checked)).toEqual([
				true,
				false,
			]);
			expect(tree.blocks[4]).toMatchObject({ ordered: true, start: 3 });
			expect(tree.blocks[5].align).toEqual(['left', 'right']);
			expect(tree.blocks[6]).toEqual({
				type: 'code',
				text: 'const a = 1;',
				lang: 'js',
			});
			expect(collect(tree, 'strong')).toHaveLength(1);
			expect(collect(tree, 'del')).toHaveLength(1);
		});
	});

	describe('entities', () => {
		it('decodes entities in text but not in code', () => {
			expect(
				firstInline(
					md('a &amp; b &lt;c&gt; &#65;&#x42; &#0; &#xD800; &amp;lt;'),
				)[0].text,
			).toBe('a & b <c> AB &#0; &#xD800; &lt;');
			expect(firstInline(md('`a < b & c`'))[0]).toEqual({
				type: 'codespan',
				text: 'a < b & c',
			});
			expect(md('```\n<div>&amp;</div>\n```').blocks[0].text).toBe(
				'<div>&amp;</div>',
			);
		});
	});

	describe('untrusted content', () => {
		const payloads = [
			'<script>alert(1)</script>',
			'text <script>alert(1)</script> text',
			'[x](javascript:alert(1))',
			'[x](JAVASCRIPT:alert(1))',
			'<javascript:alert(1)>',
			'[x](data:text/html,<script>alert(1)</script>)',
			'[x](http://allowed.example)',
			'[x](https://undeclared.example)',
			'![a](https://allowed.example/i.png)',
			'para <img src=x onerror=alert(1)> end',
			'| h |\n|---|\n| <img src=x onerror=alert(1)> |',
			'[<img src=x onerror=alert(1)>](https://allowed.example/p)',
			'<a href="javascript:alert(1)">x</a>',
			'<div onclick="alert(1)">\nblock\n</div>',
		];

		it.each(payloads)('stays inert: %s', (payload) => {
			const tree = md(payload);
			expect(tree).not.toBeNull();
			expect(collect(tree, 'html')).toHaveLength(0);
			expect(collect(tree, 'image')).toHaveLength(0);
			const urls = collect(tree, 'link').map((l) => l.url);
			for (const url of urls) {
				expect(isLinkAllowed(url, LINKS)).toBe(true);
			}
			expect(JSON.stringify(tree).toLowerCase()).not.toMatch(
				/"url":"javascript:/,
			);
		});

		it('keeps raw html as visible text', () => {
			const tree = md('<script>alert(1)</script>');
			expect(tree.blocks).toEqual([
				{
					type: 'paragraph',
					children: [{ type: 'text', text: '<script>alert(1)</script>' }],
				},
			]);
			const inline = firstInline(md('a <b>x</b>'));
			expect(inline.map((n: any) => n.text).join('')).toBe('a <b>x</b>');
		});

		it('turns only https links to declared domains into links', () => {
			const link = (source: string) => collect(md(source), 'link');
			expect(link('[x](https://allowed.example/p)')).toEqual([
				{
					type: 'link',
					url: 'https://allowed.example/p',
					host: 'allowed.example',
					children: [{ type: 'text', text: 'x' }],
				},
			]);
			expect(link('https://allowed.example')).toHaveLength(1);
			expect(link('<https://allowed.example/a>')).toHaveLength(1);
			expect(link('[x](http://allowed.example)')).toHaveLength(0);
			expect(link('[x](https://undeclared.example)')).toHaveLength(0);
			expect(link('[x](javascript:alert(1))')).toHaveLength(0);
			expect(link('[x](JAVASCRIPT:alert(1))')).toHaveLength(0);
			expect(link('<javascript:alert(1)>')).toHaveLength(0);
			expect(link('[x](https://allowed.example/p)')).toHaveLength(1);
			expect(
				collect(md('[x](https://allowed.example/p)', 7, NO_LINKS), 'link'),
			).toHaveLength(0);
			expect(firstInline(md('[x](https://undeclared.example)'))).toEqual([
				{ type: 'text', text: 'x' },
			]);
		});

		it('shows the alt text of an image', () => {
			expect(firstInline(md('![a](https://allowed.example/i.png)'))).toEqual([
				{ type: 'text', text: 'a' },
			]);
		});
	});
});

describe('collapse, tabs, segmented, buttons and icons (API 1.8)', () => {
	const t8 = (value: unknown, minor = 8) =>
		sanitizeTree(value, NO_LINKS, minor) as any;
	const btn = { type: 'button', text: 'Go', command: 'p.go' };

	describe('version gating', () => {
		it.each([
			{ type: 'collapse', title: 'T', children: [] },
			{ type: 'tabs', tabs: [{ id: 'a', title: 'A', children: [] }] },
			{
				type: 'segmented',
				options: [
					{ value: 'a', text: 'A' },
					{ value: 'b', text: 'B' },
				],
				value: 'a',
				command: 'p.go',
			},
			{ type: 'buttonGroup', children: [btn] },
			{ type: 'icon', name: 'check' },
		])('drops %j below 1.8 and keeps it at 1.8', (node) => {
			expect(t8(node, 7)).toBeNull();
			expect(t8(node, 8)).not.toBeNull();
		});

		it('ignores the new button and badge fields below 1.8', () => {
			expect(
				t8(
					{
						...btn,
						variant: 'danger',
						icon: 'check',
						iconOnly: true,
						disabled: true,
						loading: true,
						tooltip: 'tip',
					},
					7,
				),
			).toEqual(btn);
			expect(
				t8({ type: 'badge', text: 'b', icon: 'check', tooltip: 'tip' }, 7),
			).toEqual({ type: 'badge', text: 'b', color: 'gray' });
			expect(t8({ ...btn, variant: 'ghost' }, 7).variant).toBe('ghost');
		});
	});

	describe('collapse', () => {
		it('normalises title, open and visible', () => {
			expect(
				t8({
					type: 'collapse',
					title: ` ${'x'.repeat(300)} `,
					open: true,
					visible: 2,
					children: ['a', 'b', 'c'],
				}),
			).toMatchObject({
				type: 'collapse',
				title: 'x'.repeat(200),
				open: true,
				visible: 2,
			});
			const plain = t8({ type: 'collapse', visible: 0, children: ['a'] });
			expect(plain).toEqual({
				type: 'collapse',
				open: false,
				children: [{ type: 'text', text: 'a', tone: 'default' }],
			});
			expect(t8({ type: 'collapse', visible: 1.5, children: [] }).visible).toBe(
				undefined,
			);
		});

		it('counts toward depth', () => {
			let tree: any = 'leaf';
			for (let i = 0; i < 10; i++)
				tree = { type: 'collapse', title: 'x', children: [tree] };
			const flat = JSON.stringify(t8(tree));
			expect(flat.match(/"collapse"/g)!.length).toBeLessThanOrEqual(9);
		});
	});

	describe('tabs', () => {
		const tab = (id: unknown, title: unknown = 'T') => ({
			id,
			title,
			children: ['x'],
		});

		it('keeps at most 12 tabs, unique string ids of at most 64 chars', () => {
			const many = Array.from({ length: 20 }, (_, i) => tab(`t${i}`));
			expect(t8({ type: 'tabs', tabs: many }).tabs).toHaveLength(12);
			const tree = t8({
				type: 'tabs',
				tabs: [
					tab('a'),
					tab('a'),
					tab(5),
					tab('b'.repeat(65)),
					tab('c', 7),
					tab('d'),
				],
			});
			expect(tree.tabs.map((x: any) => x.id)).toEqual(['a', 'd']);
		});

		it('selects the requested tab, else the first', () => {
			const tabs = [tab('a'), tab('b')];
			expect(t8({ type: 'tabs', tabs, selected: 'b' }).selected).toBe('b');
			expect(t8({ type: 'tabs', tabs, selected: 'zz' }).selected).toBe('a');
			expect(t8({ type: 'tabs', tabs }).selected).toBe('a');
		});

		it('clamps titles and badges and drops the node without tabs', () => {
			const tree = t8({
				type: 'tabs',
				tabs: [{ id: 'a', title: 'x'.repeat(500), badge: 'y'.repeat(100) }],
			});
			expect(tree.tabs[0].title).toHaveLength(200);
			expect(tree.tabs[0].badge).toHaveLength(40);
			expect(tree.tabs[0].children).toEqual([]);
			expect(t8({ type: 'tabs', tabs: [] })).toBeNull();
		});
	});

	describe('segmented', () => {
		const seg = (options: unknown, extra: object = {}) => ({
			type: 'segmented',
			options,
			value: 'a',
			command: 'p.set',
			...extra,
		});
		const o = (...values: string[]) =>
			values.map((value) => ({ value, text: value.toUpperCase() }));

		it('needs 2 to 4 options', () => {
			expect(t8(seg(o('a')))).toBeNull();
			expect(t8(seg(o('a', 'b', 'c', 'd', 'e')))).toBeNull();
			expect(t8(seg(o('a', 'b'))).options).toHaveLength(2);
			expect(t8(seg(o('a', 'b', 'c', 'd'))).options).toHaveLength(4);
		});

		it('drops duplicate values and nodes left with fewer than 2 options', () => {
			expect(
				t8(seg(o('a', 'a', 'b'))).options.map((x: any) => x.value),
			).toEqual(['a', 'b']);
			expect(t8(seg(o('a', 'a')))).toBeNull();
			expect(t8(seg([{ value: 1, text: 'x' }, ...o('a')]))).toBeNull();
		});

		it('validates command and args like a button', () => {
			expect(t8(seg(o('a', 'b'), { command: '' }))).toBeNull();
			expect(t8(seg(o('a', 'b'), { command: undefined }))).toBeNull();
			expect(
				t8(seg(o('a', 'b'), { args: { big: 'x'.repeat(9000) } })),
			).toBeNull();
			expect(t8(seg(o('a', 'b'), { args: { k: 1 } })).args).toEqual({ k: 1 });
		});

		it('only keeps a value that is one of its options', () => {
			expect(t8(seg(o('a', 'b'), { value: 'b' })).value).toBe('b');
			expect(t8(seg(o('a', 'b'), { value: 'zz' })).value).toBeNull();
		});
	});

	describe('buttons', () => {
		it('accepts the new variants and ignores unknown ones', () => {
			for (const variant of ['danger', 'success', 'outline', 'primary']) {
				expect(t8({ ...btn, variant }).variant).toBe(variant);
			}
			expect(t8({ ...btn, variant: 'neon' }).variant).toBeUndefined();
		});

		it('carries icon, iconOnly, disabled, loading and tooltip', () => {
			expect(
				t8({
					...btn,
					icon: 'trash',
					iconOnly: true,
					disabled: true,
					loading: true,
					tooltip: ` ${'t'.repeat(300)} `,
				}),
			).toEqual({
				...btn,
				type: 'button',
				icon: 'trash',
				iconOnly: true,
				disabled: true,
				loading: true,
				tooltip: 't'.repeat(200),
			});
		});

		it('iconOnly needs a known icon, unknown icons are dropped but text stays', () => {
			const noIcon = t8({ ...btn, iconOnly: true });
			expect(noIcon.iconOnly).toBeUndefined();
			const unknown = t8({ ...btn, icon: 'rocket', iconOnly: true });
			expect(unknown.icon).toBeUndefined();
			expect(unknown.iconOnly).toBeUndefined();
			expect(unknown.text).toBe('Go');
			expect(t8({ ...btn, disabled: 'yes', loading: 1 })).toEqual({
				...btn,
				type: 'button',
			});
		});

		it('buttonGroup keeps only buttons, at most 12', () => {
			const group = t8({
				type: 'buttonGroup',
				children: [btn, 'text', { type: 'badge', text: 'b' }, btn],
			});
			expect(group.children.map((c: any) => c.type)).toEqual([
				'button',
				'button',
			]);
			const many = t8({
				type: 'buttonGroup',
				children: Array.from({ length: 20 }, () => btn),
			});
			expect(many.children).toHaveLength(12);
			expect(
				t8({ type: 'buttonGroup', children: [{ type: 'divider' }] }),
			).toBeNull();
		});
	});

	describe('badge and icon', () => {
		it('badge keeps a known icon and a clamped tooltip', () => {
			expect(
				t8({
					type: 'badge',
					text: 'b',
					icon: 'star',
					tooltip: 'x'.repeat(300),
				}),
			).toMatchObject({ icon: 'star', tooltip: 'x'.repeat(200) });
			expect(
				t8({ type: 'badge', text: 'b', icon: 'nope' }).icon,
			).toBeUndefined();
		});

		it('icon node accepts every whitelisted name and rejects others', () => {
			for (const name of ICON_NAMES) {
				expect(t8({ type: 'icon', name, tone: 'danger' })).toEqual({
					type: 'icon',
					name,
					tone: 'danger',
				});
			}
			expect(t8({ type: 'icon', name: 'rocket' })).toBeNull();
			expect(t8({ type: 'icon', name: '<script>' })).toBeNull();
			expect(t8({ type: 'icon', name: 'check' }).tone).toBe('default');
		});
	});

	it('spends the shared node budget on new nodes', () => {
		const wide = (n: number) =>
			Array.from({ length: n }, () => ({
				type: 'collapse',
				children: Array.from({ length: 100 }, () => ({
					type: 'tabs',
					tabs: Array.from({ length: 12 }, (_, i) => ({
						id: `t${i}`,
						title: 'T',
						children: ['x', 'y'],
					})),
				})),
			}));
		const tree = t8({ type: 'stack', children: wide(5) });
		const rendered = JSON.stringify(tree).match(/"type"/g)!.length;
		expect(rendered).toBeLessThanOrEqual(3000);
	});
});

describe('input, select and checkbox (API 1.9)', () => {
	const t9 = (value: unknown, minor = 9) =>
		sanitizeTree(value, NO_LINKS, minor) as any;
	const input = { type: 'input', name: 'q', command: 'p.go' };
	const select = {
		type: 'select',
		name: 's',
		command: 'p.go',
		options: [{ value: 'a', text: 'A' }],
	};
	const checkbox = { type: 'checkbox', name: 'c', label: 'L', command: 'p.go' };

	it.each([input, select, checkbox])(
		'drops %j at 1.8 and keeps it at 1.9',
		(node) => {
			expect(t9(node, 8)).toBeNull();
			expect(t9(node, 9)).not.toBeNull();
		},
	);

	it.each([input, select, checkbox])(
		'rejects bad names, missing commands and bad args for %j',
		(node) => {
			for (const name of ['', 'a b', 'x'.repeat(65), 'a<b', 5, undefined]) {
				expect(t9({ ...node, name })).toBeNull();
			}
			expect(t9({ ...node, name: 'A_b.c-1' })).not.toBeNull();
			expect(t9({ ...node, command: '' })).toBeNull();
			expect(t9({ ...node, command: undefined })).toBeNull();
			expect(t9({ ...node, args: { big: 'x'.repeat(9000) } })).toBeNull();
			expect(t9({ ...node, args: { k: 1 } }).args).toEqual({ k: 1 });
		},
	);

	it('does not pass the name or unknown fields to the node', () => {
		const out = t9({
			...input,
			type: 'input',
			inputType: 'password',
			autocomplete: 'on',
			id: 'x',
			html: '<b>',
		});
		expect(Object.keys(out).sort()).toEqual([
			'command',
			'debounce',
			'mode',
			'name',
			'type',
			'value',
		]);
		expect(JSON.stringify(t9({ ...input, type: 'password' }))).toBe('null');
	});

	describe('input', () => {
		it('clamps value and placeholder', () => {
			const out = t9({
				...input,
				value: 'v'.repeat(900),
				placeholder: 'p'.repeat(900),
			});
			expect(out.value).toHaveLength(500);
			expect(out.placeholder).toHaveLength(200);
			expect(t9(input).value).toBe('');
			expect(t9({ ...input, value: 5 }).value).toBe('');
		});

		it('defaults the mode and clamps the debounce', () => {
			expect(t9(input).mode).toBe('change');
			expect(t9({ ...input, mode: 'weird' }).mode).toBe('change');
			expect(t9({ ...input, mode: 'submit' }).mode).toBe('submit');
			expect(t9(input).debounce).toBe(300);
			expect(t9({ ...input, debounce: 1 }).debounce).toBe(150);
			expect(t9({ ...input, debounce: 99999 }).debounce).toBe(2000);
			expect(t9({ ...input, debounce: 500 }).debounce).toBe(500);
			expect(t9({ ...input, debounce: 'fast' }).debounce).toBe(300);
			expect(t9({ ...input, debounce: NaN }).debounce).toBe(300);
		});
	});

	describe('select', () => {
		const opts = (n: number) =>
			Array.from({ length: n }, (_, i) => ({ value: `v${i}`, text: `T${i}` }));

		it('keeps at most 100 options and drops repeated values', () => {
			expect(t9({ ...select, options: opts(150) }).options).toHaveLength(100);
			expect(
				t9({
					...select,
					options: [
						{ value: 'a', text: 'A' },
						{ value: 'a', text: 'B' },
						{ value: 'b', text: 'C' },
					],
				}).options.map((o: any) => o.value),
			).toEqual(['a', 'b']);
		});

		it('is dropped without usable options', () => {
			expect(t9({ ...select, options: [] })).toBeNull();
			expect(t9({ ...select, options: 'a' })).toBeNull();
			expect(t9({ ...select, options: [{ value: 1, text: 'x' }] })).toBeNull();
		});

		it('only keeps a value that is one of its options', () => {
			expect(t9({ ...select, value: 'a' }).value).toBe('a');
			expect(t9({ ...select, value: 'zz' }).value).toBeNull();
		});

		it('clamps the placeholder', () => {
			expect(
				t9({ ...select, placeholder: 'p'.repeat(900) }).placeholder,
			).toHaveLength(200);
		});
	});

	describe('checkbox', () => {
		it('needs a label and defaults to unchecked', () => {
			expect(t9({ ...checkbox, label: undefined })).toBeNull();
			expect(t9(checkbox).checked).toBe(false);
			expect(t9({ ...checkbox, checked: 'yes' }).checked).toBe(false);
			expect(t9({ ...checkbox, checked: true }).checked).toBe(true);
		});
	});

	it('spends the shared node budget', () => {
		const many = Array.from({ length: 3500 }, () => input);
		const out = t9({ type: 'stack', children: many });
		expect(out.children.length).toBeLessThan(3500);
	});
});

describe('feeds and data nodes (API 1.10)', () => {
	const t10 = (value: unknown, minor = 10) =>
		sanitizeTree(value, NO_LINKS, minor) as any;
	const col = { key: 'a', title: 'A' };
	const table = (
		extra: object = {},
		row: object = {},
		column: object = {},
	) => ({
		type: 'table',
		columns: [{ ...col, ...column }],
		rows: [{ cells: { a: 'x' }, ...row }],
		...extra,
	});

	describe('gating', () => {
		it.each([
			{ type: 'timeline', items: [{ at: 'now', text: 't' }] },
			{ type: 'messages', items: [{ author: 'a', text: 't' }] },
			{ type: 'sparkline', values: [1, 2] },
			{ type: 'empty', title: 'x' },
			{ type: 'skeleton' },
		])('drops $type at minor 9', (node) => {
			expect(t10(node, 9)).toBeNull();
			expect(t10(node)).not.toBeNull();
		});

		it('ignores the new table fields at minor 9', () => {
			const out = t10(
				table(
					{},
					{
						color: 'red',
						sort: { a: 1 },
						onClick: { command: 'p.go' },
						expand: ['x'],
					},
					{ align: 'right', width: 'sm', sortable: true },
				),
				9,
			);
			expect(out.columns[0]).toEqual(col);
			expect(out.rows[0]).toEqual({ cells: { a: expect.any(Object) } });
		});
	});

	describe('timeline', () => {
		const item = { at: '2026-01-01', title: 'T', text: 'body', tone: 'green' };
		it('keeps valid items and defaults the tone', () => {
			const out = t10({
				type: 'timeline',
				items: [item, { at: 'x', text: 'y', tone: 'neon' }],
			});
			expect(out.items[0]).toEqual(item);
			expect(out.items[1].tone).toBe('gray');
		});

		it('drops items without at or text', () => {
			const out = t10({
				type: 'timeline',
				items: [{ text: 'y' }, { at: 'x' }, { at: 5, text: 'y' }, item],
			});
			expect(out.items).toHaveLength(1);
		});

		it('clamps strings and the item count', () => {
			const out = t10({
				type: 'timeline',
				items: Array.from({ length: 300 }, () => ({
					at: 'a'.repeat(100),
					title: 't'.repeat(500),
					text: 'x'.repeat(5000),
				})),
			});
			expect(out.items).toHaveLength(200);
			expect(out.items[0].at).toHaveLength(64);
			expect(out.items[0].title).toHaveLength(200);
			expect(out.items[0].text).toHaveLength(1000);
		});
	});

	describe('messages', () => {
		it('defaults the side and clamps author and text', () => {
			const out = t10({
				type: 'messages',
				items: [
					{ author: 'a'.repeat(200), text: 'x'.repeat(5000), at: 'now' },
					{ author: 'b', text: 'y', side: 'right' },
					{ author: 'c', text: 'z', side: 'middle' },
				],
			});
			expect(out.items[0].author).toHaveLength(64);
			expect(out.items[0].text).toHaveLength(1000);
			expect(out.items.map((i: any) => i.side)).toEqual([
				'left',
				'right',
				'left',
			]);
			expect(out.items[1].at).toBeUndefined();
		});

		it('caps at 200 items', () => {
			expect(
				t10({ type: 'messages', items: [{ text: 'y' }, { author: 'a' }] })
					.items,
			).toEqual([]);
			const out = t10({
				type: 'messages',
				items: [
					...Array.from({ length: 250 }, () => ({ author: 'a', text: 'b' })),
				],
			});
			expect(out.items).toHaveLength(200);
		});
	});

	describe('table extensions', () => {
		it('keeps valid align, width and sortable and drops unknown values', () => {
			const ok = t10(
				table({}, {}, { align: 'center', width: 'lg', sortable: true }),
			);
			expect(ok.columns[0]).toMatchObject({
				align: 'center',
				width: 'lg',
				sortable: true,
			});
			const bad = t10(
				table({}, {}, { align: 'top', width: 9999, sortable: 'yes' }),
			);
			expect(bad.columns[0]).toEqual(col);
		});

		it('keeps a row color from the closed list only', () => {
			expect(t10(table({}, { color: 'red' })).rows[0].color).toBe('red');
			expect(t10(table({}, { color: 'pink' })).rows[0].color).toBeUndefined();
		});

		it('validates the row onClick like a button command', () => {
			expect(
				t10(table({}, { onClick: { command: 'p.go', args: { id: 1 } } }))
					.rows[0].onClick,
			).toEqual({ command: 'p.go', args: { id: 1 } });
			for (const onClick of [
				{},
				{ command: '' },
				{ command: 5 },
				'p.go',
				null,
				{ command: 'p.go', args: 'x'.repeat(9000) },
			]) {
				expect(t10(table({}, { onClick })).rows[0].onClick).toBeUndefined();
			}
		});

		it('keeps task rows', () => {
			expect(t10(table({}, { taskId: 7 })).rows[0].taskId).toBe(7);
		});

		it('keeps sort values for known columns, finite numbers and clamped strings', () => {
			const out = t10(table({}, { sort: { a: 5, b: 1, c: 'x'.repeat(500) } }));
			expect(out.rows[0].sort).toEqual({ a: 5 });
			expect(
				t10(table({}, { sort: { a: 'y'.repeat(500) } })).rows[0].sort.a,
			).toHaveLength(200);
			expect(
				t10(table({}, { sort: { a: Infinity } })).rows[0].sort,
			).toBeUndefined();
			expect(t10(table({}, { sort: { a: {} } })).rows[0].sort).toBeUndefined();
		});

		it('sanitizes the expand subtree', () => {
			const out = t10(
				table(
					{},
					{
						expand: [
							{ type: 'text', text: 'hi' },
							{ type: 'script', src: 'x' },
							'plain',
						],
					},
				),
			);
			expect(out.rows[0].expand.map((n: any) => n.type)).toEqual([
				'text',
				'text',
			]);
		});

		it('limits the depth of expanded nodes', () => {
			let deep: any = { type: 'text', text: 'leaf' };
			for (let i = 0; i < 12; i++) deep = { type: 'stack', children: [deep] };
			const out = t10(table({}, { expand: [deep] }));
			let depth = 0;
			let cur = out.rows[0].expand[0];
			while (cur?.children?.[0]) {
				cur = cur.children[0];
				depth++;
			}
			expect(depth).toBeLessThan(8);
			expect(JSON.stringify(out)).not.toContain('leaf');
		});

		it('counts rows and expanded nodes against the shared budget', () => {
			const rows = Array.from({ length: 500 }, () => ({
				cells: { a: 'x' },
				expand: Array.from({ length: 20 }, () => 'e'),
			}));
			const out = t10({
				type: 'stack',
				children: Array.from({ length: 5 }, () => table({ rows })),
			});
			const rendered = JSON.stringify(out).match(/"type"/g)!.length;
			expect(rendered).toBeLessThanOrEqual(3000);
		});
	});

	describe('sparkline', () => {
		it('drops non-finite values and caps at 365', () => {
			const out = t10({
				type: 'sparkline',
				values: [1, NaN, Infinity, 'x', null, 2, ...Array(500).fill(3)],
			});
			expect(out.values).toHaveLength(365);
			expect(out.values.slice(0, 2)).toEqual([1, 2]);
		});

		it('defaults the color, clamps the label and accepts no values', () => {
			const out = t10({
				type: 'sparkline',
				values: 'nope',
				color: 'neon',
				label: 'l'.repeat(500),
			});
			expect(out.values).toEqual([]);
			expect(out.color).toBe('blue');
			expect(out.label).toHaveLength(200);
			expect(t10({ type: 'sparkline', values: [1], color: 'red' }).color).toBe(
				'red',
			);
		});

		it('spends the shared budget per value', () => {
			const out = t10({
				type: 'stack',
				children: Array.from({ length: 10 }, () => ({
					type: 'sparkline',
					values: Array(365).fill(1),
				})),
			});
			const total = out.children.reduce(
				(sum: number, c: any) => sum + c.values.length,
				0,
			);
			expect(total).toBeLessThan(3000);
		});
	});

	describe('empty', () => {
		it('needs a title and whitelists the icon', () => {
			expect(t10({ type: 'empty' })).toBeNull();
			expect(t10({ type: 'empty', title: '  ' })).toBeNull();
			const out = t10({
				type: 'empty',
				title: 'T'.repeat(500),
				text: 'x'.repeat(5000),
				icon: 'info',
			});
			expect(out.title).toHaveLength(200);
			expect(out.text).toHaveLength(1000);
			expect(out.icon).toBe('info');
			expect(
				t10({ type: 'empty', title: 'T', icon: 'nope' }).icon,
			).toBeUndefined();
		});
	});

	describe('skeleton', () => {
		it('defaults the variant and clamps lines', () => {
			expect(t10({ type: 'skeleton' })).toEqual({
				type: 'skeleton',
				variant: 'text',
				lines: 3,
			});
			expect(t10({ type: 'skeleton', variant: 'x' }).variant).toBe('text');
			expect(t10({ type: 'skeleton', variant: 'table' }).variant).toBe('table');
			expect(t10({ type: 'skeleton', lines: 99 }).lines).toBe(12);
			expect(t10({ type: 'skeleton', lines: 0 }).lines).toBe(1);
			expect(t10({ type: 'skeleton', lines: NaN }).lines).toBe(3);
		});
	});
});

describe('textarea and date (API 1.11)', () => {
	const t11 = (value: unknown, minor = 11) =>
		sanitizeTree(value, NO_LINKS, minor) as any;
	const area = { type: 'textarea', name: 'note', command: 'p.go' };
	const date = { type: 'date', name: 'due', command: 'p.go' };

	it.each([area, date])('drops %j at 1.10 and keeps it at 1.11', (node) => {
		expect(t11(node, 10)).toBeNull();
		expect(t11(node, 11)).not.toBeNull();
	});

	it.each([area, date])(
		'rejects bad names, commands and args for %j',
		(node) => {
			for (const name of ['', 'a b', 'x'.repeat(65), 5, undefined]) {
				expect(t11({ ...node, name })).toBeNull();
			}
			expect(t11({ ...node, command: '' })).toBeNull();
			expect(t11({ ...node, args: { big: 'x'.repeat(9000) } })).toBeNull();
			expect(t11({ ...node, args: { k: 1 } }).args).toEqual({ k: 1 });
		},
	);

	it('exports the textarea cap', () => {
		expect(MAX_TEXTAREA_VALUE).toBe(5000);
	});

	describe('textarea', () => {
		it('defaults and drops unknown fields', () => {
			expect(
				t11({ ...area, type: 'textarea', html: '<b>', inputType: 'x' }),
			).toEqual({
				type: 'textarea',
				name: 'note',
				value: '',
				rows: 4,
				command: 'p.go',
			});
		});

		it('cuts the value to 5000 code points without splitting a pair', () => {
			const out = t11({ ...area, value: '😀'.repeat(6000) }).value;
			expect(Array.from(out)).toHaveLength(5000);
			expect(out).toBe('😀'.repeat(5000));
			expect(t11({ ...area, value: '漢'.repeat(5000) }).value).toHaveLength(
				5000,
			);
			expect(t11({ ...area, value: 5 }).value).toBe('');
		});

		it('keeps newlines in the value', () => {
			expect(t11({ ...area, value: 'a\nb' }).value).toBe('a\nb');
		});

		it('clamps rows to 2..12', () => {
			expect(t11({ ...area, rows: 1 }).rows).toBe(2);
			expect(t11({ ...area, rows: 99 }).rows).toBe(12);
			expect(t11({ ...area, rows: 7 }).rows).toBe(7);
			expect(t11({ ...area, rows: 5.4 }).rows).toBe(5);
			expect(t11({ ...area, rows: '7' }).rows).toBe(4);
			expect(t11({ ...area, rows: NaN }).rows).toBe(4);
		});

		it('clamps the placeholder and submit text', () => {
			expect(
				Array.from(
					t11({ ...area, placeholder: '\u{1F600}'.repeat(201) }).placeholder,
				),
			).toHaveLength(200);
			expect(
				t11({ ...area, submitText: 's'.repeat(90) }).submitText,
			).toHaveLength(40);
			expect(t11({ ...area, submitText: '  ' })).not.toHaveProperty(
				'submitText',
			);
			expect(t11({ ...area, submitText: 5 })).not.toHaveProperty('submitText');
			expect(t11({ ...area, submitText: ' Post ' }).submitText).toBe('Post');
		});
	});

	describe('date', () => {
		it('keeps real calendar dates only', () => {
			expect(t11({ ...date, value: '2026-02-28' }).value).toBe('2026-02-28');
			expect(t11({ ...date, value: '2024-02-29' }).value).toBe('2024-02-29');
			for (const value of [
				'2026-02-30',
				'2025-02-29',
				'2026-13-01',
				'2026-00-10',
				'2026-04-31',
				'0999-12-31',
				'10000-01-01',
				'2026-1-5',
				'2026/01/05',
				'2026-01-05T00:00',
				' 2026-01-05',
				5,
				null,
			]) {
				expect(t11({ ...date, value }).value).toBeNull();
			}
			expect(t11({ ...date, value: '1000-01-01' }).value).toBe('1000-01-01');
			expect(t11({ ...date, value: '9999-12-31' }).value).toBe('9999-12-31');
		});

		it('defaults to no value and no bounds', () => {
			expect(t11(date)).toEqual({
				type: 'date',
				name: 'due',
				value: null,
				command: 'p.go',
			});
		});

		it('drops invalid min and max', () => {
			const out = t11({ ...date, min: '2026-02-30', max: '2026-03-01' });
			expect(out).not.toHaveProperty('min');
			expect(out.max).toBe('2026-03-01');
		});

		it('drops both bounds when min is after max', () => {
			const out = t11({ ...date, min: '2026-05-01', max: '2026-04-01' });
			expect(out).not.toHaveProperty('min');
			expect(out).not.toHaveProperty('max');
			expect(t11({ ...date, min: '2026-04-01', max: '2026-04-01' }).min).toBe(
				'2026-04-01',
			);
		});

		it('nulls a value outside the bounds', () => {
			const bounds = { min: '2026-01-10', max: '2026-01-20' };
			expect(t11({ ...date, ...bounds, value: '2026-01-09' }).value).toBeNull();
			expect(t11({ ...date, ...bounds, value: '2026-01-21' }).value).toBeNull();
			expect(t11({ ...date, ...bounds, value: '2026-01-10' }).value).toBe(
				'2026-01-10',
			);
			expect(t11({ ...date, ...bounds, value: '2026-01-20' }).value).toBe(
				'2026-01-20',
			);
		});
	});

	it('helpers cut and validate directly', () => {
		expect(cutCodePoints('a😀b', 2)).toBe('a😀');
		expect(cutCodePoints('a😀b', 1)).toBe('a');
		expect(cutCodePoints('ab', 5)).toBe('ab');
		expect(calendarDate('2026-12-31')).toBe('2026-12-31');
		expect(calendarDate('2026-12-32')).toBeNull();
	});

	it('spends the shared node budget and depth', () => {
		const many = Array.from({ length: 3500 }, () => area);
		expect(t11({ type: 'stack', children: many }).children.length).toBeLessThan(
			3500,
		);
		let nested: any = date;
		for (let i = 0; i < 9; i++) nested = { type: 'stack', children: [nested] };
		expect(JSON.stringify(t11(nested))).not.toContain('"date"');
	});
});

describe('chart node (API 1.12)', () => {
	const t12 = (value: unknown, minor = 12) =>
		sanitizeTree(value, NO_LINKS, minor) as any;
	const chart = (extra: object = {}) => ({
		type: 'chart',
		labels: ['a', 'b', 'c'],
		series: [{ name: 'S', values: [1, 2, 3] }],
		...extra,
	});

	it('is dropped before minor 12', () => {
		expect(t12(chart(), 11)).toBeNull();
		expect(t12(chart(), 10)).toBeNull();
		expect(t12(chart())).not.toBeNull();
	});

	it('applies defaults', () => {
		expect(t12(chart())).toEqual({
			type: 'chart',
			kind: 'bar',
			labels: ['a', 'b', 'c'],
			series: [{ name: 'S', values: [1, 2, 3], color: 'blue' }],
			height: 200,
		});
		expect(t12(chart({ kind: 'pie' })).kind).toBe('bar');
		expect(t12(chart({ kind: 'line' })).kind).toBe('line');
	});

	it('picks default colors per series and keeps valid ones', () => {
		const out = t12(
			chart({
				series: [
					{ name: 'A', values: [1] },
					{ name: 'B', values: [1], color: 'red' },
					{ name: 'C', values: [1], color: 'neon' },
				],
			}),
		);
		expect(out.series.map((s: any) => s.color)).toEqual([
			'blue',
			'red',
			'green',
		]);
	});

	it('stringifies and caps labels', () => {
		const out = t12(
			chart({
				labels: [1.5, 'x'.repeat(100), null, {}, ...Array(400).fill('l')],
				series: [{ name: 'S', values: [1] }],
			}),
		);
		expect(out.labels).toHaveLength(365);
		expect(out.labels.slice(0, 4)).toEqual(['1.5', 'x'.repeat(64), '', '']);
	});

	it('turns bad values into gaps, pads and cuts to the labels', () => {
		const out = t12(
			chart({
				series: [
					{ name: 'A', values: [1, NaN, 'x', Infinity, 5] },
					{ name: 'B', values: [1] },
				],
			}),
		);
		expect(out.series[0].values).toEqual([1, null, null]);
		expect(out.series[1].values).toEqual([1, null, null]);
	});

	it('numbers the labels when none are given', () => {
		const out = t12({
			type: 'chart',
			series: [
				{ name: 'A', values: [1, 2] },
				{ name: 'B', values: [1, 2, 3, 4] },
			],
		});
		expect(out.labels).toEqual(['1', '2', '3', '4']);
		expect(out.series[0].values).toEqual([1, 2, null, null]);
	});

	it('limits series to 8 and requires a name', () => {
		const out = t12(
			chart({
				series: [
					{ values: [1] },
					{ name: '  ', values: [1] },
					'x',
					...Array.from({ length: 12 }, (_, i) => ({
						name: `${i}`.repeat(100),
						values: [1],
					})),
				],
			}),
		);
		expect(out.series).toHaveLength(8);
		expect(out.series[0].name).toHaveLength(64);
	});

	it('drops the node without series or points', () => {
		expect(t12(chart({ series: [] }))).toBeNull();
		expect(t12(chart({ series: [{ values: [1] }] }))).toBeNull();
		expect(
			t12({ type: 'chart', series: [{ name: 'A', values: [] }] }),
		).toBeNull();
		expect(
			t12(chart({ labels: [], series: [{ name: 'A', values: [] }] })),
		).toBeNull();
	});

	it('clamps the height and the label', () => {
		expect(t12(chart({ height: 10 })).height).toBe(120);
		expect(t12(chart({ height: 9000 })).height).toBe(400);
		expect(t12(chart({ height: 'tall' })).height).toBe(200);
		expect(t12(chart({ label: 'l'.repeat(500) })).label).toHaveLength(200);
	});

	it('turns values beyond 1e15 into gaps', () => {
		const out = t12(
			chart({
				labels: ['a', 'b', 'c', 'd'],
				series: [{ name: 'S', values: [1e15, 1.1e15, -1e308, -1e15] }],
			}),
		);
		expect(out.series[0].values).toEqual([1e15, null, null, -1e15]);
	});

	it('drops a chart cut short by the node budget', () => {
		const big = () =>
			chart({
				labels: Array(100).fill('l'),
				series: Array.from({ length: 8 }, (_, i) => ({
					name: `s${i}`,
					values: Array(100).fill(1),
				})),
			});
		const out = t12({
			type: 'stack',
			children: Array.from({ length: 4 }, big),
		});
		expect(out.children.length).toBe(3);
		for (const c of out.children) {
			expect(c.series.every((s: any) => !s.values.includes(null))).toBe(true);
		}
		const alone = t12({
			type: 'stack',
			children: [
				{ type: 'text', text: 'x' },
				...Array.from({ length: 2990 }, () => ({ type: 'divider' })),
				chart(),
			],
		});
		expect(alone.children.some((n: any) => n.type === 'chart')).toBe(false);
	});

	it('spends the shared budget on labels, series and values', () => {
		const big = () =>
			chart({
				labels: Array(365).fill('l'),
				series: Array.from({ length: 8 }, (_, i) => ({
					name: `s${i}`,
					values: Array(365).fill(1),
				})),
			});
		const out = t12({
			type: 'stack',
			children: Array.from({ length: 3 }, big),
		});
		const used = out.children.reduce(
			(sum: number, c: any) =>
				sum +
				1 +
				c.labels.length +
				c.series.length +
				c.series.reduce(
					(n: number, s: any) =>
						n + s.values.filter((v: unknown) => v !== null).length,
					0,
				),
			0,
		);
		expect(used).toBeLessThanOrEqual(3000);
		expect(out.children.length).toBeLessThan(3);
	});
});
