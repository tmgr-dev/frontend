import {
	editorJsToMarkdown,
	exportWorkspaceFiles,
	taskFileName,
	taskToMarkdown,
	type ExportTask,
} from '../markdown';

const task = (overrides: Partial<ExportTask> = {}): ExportTask => ({
	id: 7,
	title: 'Ship the local workspace',
	description: 'Plain *markdown* body',
	description_json: null,
	status: { name: 'In progress', type: 'active' },
	category: { title: 'Taskmgr', code: 'TM' },
	category_tasks_sequence_id: 12,
	priority: 'high',
	common_time: 5400,
	approximately_time: 7200,
	checkpoints: [
		{ description: 'schema', checked: true },
		{ description: 'UI', checked: false },
	],
	created_at: '2026-09-26T10:00:00Z',
	updated_at: '2026-09-26T12:00:00Z',
	comments: [
		{
			message: 'Looks good',
			created_at: '2026-09-26T11:00:00Z',
			author: 'Yurij',
		},
	],
	files: [{ name: 'notes.png', file_path: 'abc/notes.png' }],
	...overrides,
});

it('writes a task with frontmatter, checkpoints, attachments and comments', () => {
	const md = taskToMarkdown(task(), '../files');
	expect(md).toContain(
		'---\nkey: TM-12\ntitle: "Ship the local workspace"\nstatus: "In progress"',
	);
	expect(md).toContain('time_spent: 1h 30m');
	expect(md).toContain('estimate: 2h 0m');
	expect(md).toContain(
		'# TM-12: Ship the local workspace\n\nPlain *markdown* body',
	);
	expect(md).toContain('- [x] schema\n- [ ] UI');
	expect(md).toContain('- [notes.png](../files/abc/notes.png)');
	expect(md).toContain('**Yurij** · 2026-09-26T11:00:00Z\n\nLooks good');
});

it('keys a task by the key its title is stamped with', () => {
	const t = task({ title: 'TM-242 example', category_tasks_sequence_id: 78 });
	expect(taskFileName(t)).toBe('TM-78-tm-242-example.md');
	expect(taskToMarkdown(t, '..')).toContain('key: TM-242\n');
});

it('escapes titles safely in YAML and names files by key and title', () => {
	const t = task({
		title: 'Fix: "quotes" & colons',
		category: null,
		category_tasks_sequence_id: null,
	});
	expect(taskToMarkdown(t, '..')).toContain(
		'title: "Fix: \\"quotes\\" & colons"',
	);
	expect(taskFileName(t)).toBe('T-7-fix-quotes-colons.md');
	expect(taskFileName(task({ title: 'Личное дело' }))).toBe(
		'TM-12-личное-дело.md',
	);
});

it('converts Editor.js descriptions', () => {
	const md = editorJsToMarkdown({
		blocks: [
			{ type: 'header', data: { text: 'Plan', level: 2 } },
			{
				type: 'paragraph',
				data: { text: 'Do <b>this</b> and <a href="https://x.dev">that</a>' },
			},
			{
				type: 'list',
				data: {
					style: 'ordered',
					items: [
						'one',
						{ content: 'two', items: [{ content: 'nested', items: [] }] },
					],
				},
			},
			{ type: 'checklist', data: { items: [{ text: 'done', checked: true }] } },
			{ type: 'code', data: { code: 'npm test' } },
		],
	});
	expect(md).toBe(
		'## Plan\n\nDo **this** and [that](https://x.dev)\n\n1. one\n2. two\n  1. nested\n\n- [x] done\n\n```\nnpm test\n```',
	);
});

it('converts list@2 descriptions and drops empty lists', () => {
	const item = (content: string, meta = {}, items: unknown[] = []) => ({
		content,
		meta,
		items,
	});
	const md = editorJsToMarkdown({
		blocks: [
			{
				type: 'list',
				data: {
					style: 'unordered',
					meta: {},
					items: [item('a', {}, [item('<b>a1</b>')])],
				},
			},
			{
				type: 'list',
				data: {
					style: 'ordered',
					meta: { counterType: 'numeric' },
					items: [item('one'), item('two')],
				},
			},
			{
				type: 'list',
				data: {
					style: 'checklist',
					meta: {},
					items: [
						item('done', { checked: true }, [item('sub', { checked: false })]),
						item('todo', { checked: false }),
					],
				},
			},
			{ type: 'checklist', data: { items: [] } },
			{ type: 'list', data: { style: 'checklist', meta: {}, items: [] } },
		],
	});
	expect(md).toBe(
		'- a\n  - **a1**\n\n1. one\n2. two\n\n- [x] done\n  - [ ] sub\n- [ ] todo',
	);
});

it('exports readable siblings of a list kept raw as malformed', () => {
	const md = editorJsToMarkdown({
		blocks: [
			{
				type: 'list',
				data: { style: 'checklist', items: { content: 'a', items: 'x' } },
			},
			{ type: 'list', data: { items: 'abc' } },
			{ type: 'checklist', data: { items: 5 } },
			{ type: 'paragraph', data: { text: 'readable' } },
		],
	});
	expect(md).toBe('readable');
	expect(
		editorJsToMarkdown({
			blocks: [
				{
					type: 'list',
					data: {
						style: 'checklist',
						items: [
							'skipped',
							{ content: {}, items: 'x' },
							{ content: 'ok', items: [7] },
						],
					},
				},
			],
		}),
	).toBe('- [ ] skipped\n- [ ] {}\n  - [ ] x\n- [ ] ok\n  - [ ] 7');
});

it('numbers ordered lists from meta.start at the top level only', () => {
	const item = (content: string, items: unknown[] = []) => ({
		content,
		meta: {},
		items,
	});
	const ordered = (start: unknown) =>
		editorJsToMarkdown({
			blocks: [
				{
					type: 'list',
					data: {
						style: 'ordered',
						meta: { start },
						items: [item('a', [item('a1')]), item('b')],
					},
				},
			],
		});
	expect(ordered(5)).toBe('5. a\n  1. a1\n6. b');
	expect(ordered(0)).toBe('1. a\n  1. a1\n2. b');
	expect(ordered('3')).toBe('1. a\n  1. a1\n2. b');
});

it('gives two tasks stamped with the same key and title separate files', () => {
	const twin = (id: number) =>
		task({
			id,
			title: 'TM-5: x',
			category: null,
			category_tasks_sequence_id: null,
		});
	const paths = exportWorkspaceFiles('Personal', [], [twin(1), twin(2)], 'now')
		.map((f) => f.path)
		.filter((p) => p.startsWith('tasks/'));
	expect(new Set(paths).size).toBe(2);
});

it('exports a workspace as a README index plus one file per task', () => {
	const files = exportWorkspaceFiles(
		'Personal',
		[{ name: 'Backlog' }, { name: 'In progress' }],
		[
			task(),
			task({
				id: 8,
				title: 'Loose',
				status: null,
				category: null,
				category_tasks_sequence_id: null,
			}),
		],
		'2026-09-26 12:00',
	);
	expect(files.map((f) => f.path)).toEqual([
		'README.md',
		'tasks/TM-12-ship-the-local-workspace.md',
		'tasks/T-8-loose.md',
	]);
	expect(files[0].content).toContain(
		'## In progress\n\n- [TM-12: Ship the local workspace](tasks/TM-12-ship-the-local-workspace.md)',
	);
	expect(files[0].content).toContain(
		'## No status\n\n- [T-8: Loose](tasks/T-8-loose.md)',
	);
	expect(files[1].content).toContain('(../../../files/abc/notes.png)');
});
