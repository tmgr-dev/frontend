import { sanitizeTree } from '../uiTree';

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
