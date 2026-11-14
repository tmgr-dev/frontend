import { createLocalApi } from '../api';
import { dispatchLocal } from '../dispatch';
import {
	LOCAL_AGENT_TOOL_LIMIT,
	localAgentTools,
	runLocalTool,
	toOpenAiTools,
} from '../localAgentTools';
import { MCP_TOOLS } from '../mcp';
import { migrate } from '../schema';
import type { LocalContext } from '../types';
import { memoryDb, nodeSqliteAvailable } from './nodeDb';

const describeSqlite = nodeSqliteAvailable ? describe : describe.skip;

const REUSED = [
	'workspace_context',
	'pages_for_task',
	'pages_search',
	'pages_tree',
	'pages_get',
	'pages_create',
	'pages_update',
	'pages_append',
	'pages_set_section',
	'get_task',
	'search_tasks',
	'list_tasks_by_status',
	'create_task',
	'update_task',
	'list_comments',
	'add_comment',
	'list_task_files',
	'list_statuses',
];
const NEW = [
	'list_tasks',
	'start_task_timer',
	'stop_task_timer',
	'create_status',
	'update_status',
];
const WRITES = [
	'pages_create',
	'pages_update',
	'pages_append',
	'pages_set_section',
	'create_task',
	'update_task',
	'add_comment',
	'start_task_timer',
	'stop_task_timer',
	'create_status',
	'update_status',
];

describe('local agent tool list', () => {
	it('exposes exactly the allowed names', () => {
		expect(localAgentTools.map((t) => t.name).sort()).toEqual(
			[...REUSED, ...NEW].sort(),
		);
	});

	it('never exposes raw requests or persona identity tools', () => {
		const names = localAgentTools.map((t) => t.name);
		for (const banned of [
			'tmgr_request',
			'whoami',
			'get_persona',
			'start_agent_work',
			'assign_persona',
			'delete_task',
		])
			expect(names).not.toContain(banned);
	});

	it('marks write tools', () => {
		expect(
			localAgentTools
				.filter((t) => t.write)
				.map((t) => t.name)
				.sort(),
		).toEqual([...WRITES].sort());
	});

	it('keeps the MCP schema for reused tools', () => {
		const openAi = toOpenAiTools();
		for (const name of REUSED) {
			const mcp = MCP_TOOLS.find((t) => t.name === name)!;
			const tool = openAi.find((t) => t.function.name === name)!;
			expect(tool.type).toBe('function');
			expect(tool.function.parameters).toEqual(mcp.inputSchema);
			expect(tool.function.description).toBe(mcp.description);
		}
	});
});

describeSqlite('local agent tools on a local workspace', () => {
	const router = createLocalApi();
	const deps = { personaPrompt: async () => null };
	const clock = new Date('2026-11-14T10:00:00Z');
	let ctx: LocalContext;
	let taskId: number;

	const run = (name: string, args: Record<string, unknown> = {}) =>
		runLocalTool(name, args, ctx, router, deps);

	beforeEach(async () => {
		ctx = {
			db: memoryDb(),
			workspace: {
				id: -1,
				name: 'Personal',
				code: 'local-personal',
				schema_version: 0,
				created_at: '',
				path: '/tmp/x',
				database: '/tmp/x/workspace.db',
			},
			user: { id: -1, name: 'Me', email: '' },
			now: () => clock,
			files: {
				url: (key) => `tmgrfile://localhost/${key}`,
				read: async () => new Blob(['x']),
				remove: async () => {},
			},
		};
		await migrate(ctx.db, clock.toISOString());
		const created = await dispatchLocal(router, ctx, 'POST', 'tasks', {
			title: 'Do the thing',
		});
		taskId = created!.data.data.id;
	});

	it('starts and stops the timer through the countdown route', async () => {
		const started = JSON.parse(await run('start_task_timer', { taskId }));
		expect(started.startTime).toBeGreaterThan(0);
		const stopped = JSON.parse(await run('stop_task_timer', { taskId }));
		expect(stopped.startTime).toBe(0);
	});

	it('creates and updates a status', async () => {
		const created = JSON.parse(
			await run('create_status', { name: 'Review', color: '#ff0000' }),
		);
		expect(created.name).toBe('Review');
		const updated = JSON.parse(
			await run('update_status', { statusId: created.id, name: 'QA' }),
		);
		expect(updated).toMatchObject({ name: 'QA', color: '#ff0000' });
		const list = JSON.parse(await run('list_statuses'));
		expect(list.map((s: any) => s.name)).toContain('QA');
	});

	it('lists tasks with pagination', async () => {
		const page = JSON.parse(await run('list_tasks', { perPage: 5 }));
		expect(page.items.map((t: any) => t.title)).toContain('Do the thing');
		expect(page).toMatchObject({ page: 1, perPage: 5, hasMore: false });
	});

	it('runs a reused tool through the user context', async () => {
		const task = JSON.parse(await run('get_task', { taskId }));
		expect(task.title).toBe('Do the thing');
		const comment = JSON.parse(
			await run('add_comment', { taskId, message: 'hi' }),
		);
		expect(comment.message).toBe('hi');
	});

	it('returns failures as text', async () => {
		expect(await run('get_task', {})).toMatch(/taskId is required/);
		expect(await run('nope')).toMatch(/Unknown tool/);
		expect(await run('start_task_timer', { taskId: 9999 })).toMatch(
			/not found|error/i,
		);
		expect(
			await run('start_task_timer', { taskId: '1/../../statuses/1' }),
		).toMatch(/taskId must be a positive integer/);
		expect(await run('update_status', { statusId: 1.5 })).toMatch(
			/statusId must be a positive integer/,
		);
	});

	it('truncates long results', async () => {
		for (let i = 0; i < 60; i++)
			await dispatchLocal(router, ctx, 'POST', 'tasks', {
				title: `Task number ${i} ${'x'.repeat(200)}`,
			});
		const text = await run('list_tasks', { perPage: 100 });
		expect(text.length).toBeLessThanOrEqual(
			LOCAL_AGENT_TOOL_LIMIT +
				'\n[result truncated — narrow your query]'.length,
		);
		expect(text.endsWith('[result truncated — narrow your query]')).toBe(true);
	});
});
