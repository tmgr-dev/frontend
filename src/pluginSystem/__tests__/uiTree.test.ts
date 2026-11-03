import {
	isLinkAllowed,
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
			expect(lengths.slice(10).every((n: number) => n === 1000)).toBe(true);
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
