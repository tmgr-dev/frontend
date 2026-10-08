import { createLocalApi } from '../api';
import { dispatchLocal } from '../dispatch';
import { handleMcpRequest } from '../mcp';
import { disableLocalPersona, enableLocalPersona } from '../personas';
import { migrate } from '../schema';
import type { LocalContext } from '../types';
import { memoryDb, nodeSqliteAvailable } from './nodeDb';

const describeSqlite = nodeSqliteAvailable ? describe : describe.skip;

describeSqlite('local MCP handler for personas', () => {
	let ctx: LocalContext;
	let taskId: number;
	const clock = new Date('2026-09-28T10:00:00Z');
	const router = createLocalApi();
	const persona = { kind: 'persona' as const, id: 'p-1', name: 'Reviewer' };
	const deps = {
		personaPrompt: async (uuid: string) =>
			uuid === 'p-1' ? { system_prompt: 'Be a careful reviewer.', prompt_version: 2 } : null,
	};

	const rpc = (body: unknown) => handleMcpRequest(router, ctx, JSON.stringify(body), deps);
	const call = (name: string, args?: Record<string, unknown>, id: number | string = 1) =>
		rpc({ jsonrpc: '2.0', id, method: 'tools/call', params: { name, arguments: args ?? {} } });

	beforeEach(async () => {
		ctx = {
			db: memoryDb(),
			workspace: { id: -1, name: 'Personal', code: 'local-personal', schema_version: 0, created_at: '', path: '/tmp/x', database: '/tmp/x/workspace.db' },
			user: { id: 7, name: 'Yurij', email: 'me@example.com' },
			now: () => clock,
			files: { url: (key) => `tmgrfile://localhost/${key}`, read: async () => new Blob(['x']), remove: async () => {} },
			actor: persona,
		};
		await migrate(ctx.db, clock.toISOString());
		await ctx.db.execute(
			`INSERT INTO personas (uuid, owner_user_id, owner_name, name, description, avatar_file, synced_at, archived_at)
			 VALUES ('p-1', 7, 'Yurij', 'Reviewer', 'Reviews PRs', NULL, ?, NULL)`,
			[clock.toISOString()],
		);
		const created = await dispatchLocal(router, { ...ctx, actor: undefined }, 'POST', 'tasks', { title: 'Do the thing' });
		taskId = created!.data.data.id;
	});

	it('answers initialize with the requested protocol version when it is supported', async () => {
		const res = await rpc({ jsonrpc: '2.0', id: 1, method: 'initialize', params: { protocolVersion: '2024-11-05' } });
		expect(res.status).toBe(200);
		const body = JSON.parse(res.body);
		expect(body.result.protocolVersion).toBe('2024-11-05');
		expect(body.result.serverInfo).toEqual({ name: 'tmgr-desktop-local', version: '1' });
		expect(body.result.capabilities).toEqual({ tools: {} });
	});

	it('falls back to the default protocol version when the client asks for an unknown one', async () => {
		const res = await rpc({ jsonrpc: '2.0', id: 1, method: 'initialize', params: { protocolVersion: '1999-01-01' } });
		const body = JSON.parse(res.body);
		expect(body.result.protocolVersion).toBe('2025-06-18');
	});

	it('returns 202 with an empty body for a notification', async () => {
		const res = await rpc({ jsonrpc: '2.0', method: 'notifications/initialized' });
		expect(res.status).toBe(202);
		expect(res.body).toBe('');
	});

	it('returns a parse error for invalid JSON', async () => {
		const res = await handleMcpRequest(router, ctx, '{not json', deps);
		expect(res.status).toBe(400);
		const body = JSON.parse(res.body);
		expect(body.error.code).toBe(-32700);
	});

	it('answers a batch of requests with a matching array of responses', async () => {
		const res = await rpc([
			{ jsonrpc: '2.0', id: 1, method: 'ping' },
			{ jsonrpc: '2.0', id: 2, method: 'tools/list' },
		]);
		expect(res.status).toBe(200);
		const body = JSON.parse(res.body);
		expect(body).toHaveLength(2);
		expect(body[0]).toEqual({ jsonrpc: '2.0', id: 1, result: {} });
		expect(body[1].result.tools.map((t: any) => t.name)).toContain('whoami');
	});

	it('lists only the tools the persona\'s grant permits, plus the always-on identity tools', async () => {
		await enableLocalPersona(ctx, 'p-1', ['tasks:read']);
		const res = await rpc({ jsonrpc: '2.0', id: 1, method: 'tools/list' });
		const names: string[] = JSON.parse(res.body).result.tools.map((t: any) => t.name);
		expect(names).toEqual(
			expect.arrayContaining(['whoami', 'get_persona', 'get_persona_skill', 'list_workspaces', 'tmgr_request', 'get_task', 'search_tasks', 'list_tasks_by_status']),
		);
		expect(names).not.toEqual(expect.arrayContaining(['create_task', 'add_comment', 'delete_task', 'list_tasks']));

		await enableLocalPersona(ctx, 'p-1', ['tasks:read', 'tasks:write', 'comments:write']);
		const res2 = await rpc({ jsonrpc: '2.0', id: 1, method: 'tools/list' });
		const names2: string[] = JSON.parse(res2.body).result.tools.map((t: any) => t.name);
		expect(names2).toEqual(expect.arrayContaining(['create_task', 'update_task', 'add_comment']));
	});

	it('never lists delete_task and refuses calling it with a tool error', async () => {
		await enableLocalPersona(ctx, 'p-1', ['tasks:read', 'tasks:write']);
		const listed = JSON.parse((await rpc({ jsonrpc: '2.0', id: 1, method: 'tools/list' })).body).result.tools;
		expect(listed.map((t: any) => t.name)).not.toContain('delete_task');

		const res = await call('delete_task', { taskId });
		const result = JSON.parse(res.body).result;
		expect(result.isError).toBe(true);
		expect(result.content[0].text).toBe('Tool not available to personas');
	});

	it('creates a task and adds a comment as the persona, attributed to it', async () => {
		await enableLocalPersona(ctx, 'p-1', ['tasks:write', 'comments:write', 'comments:read']);
		const created = JSON.parse((await call('create_task', { title: 'From the persona' })).body);
		const newTaskId = JSON.parse(created.result.content[0].text).id;
		expect(newTaskId).toBeGreaterThan(0);

		await call('add_comment', { taskId: newTaskId, message: 'Looks fine' });
		const listed = JSON.parse((await call('list_comments', { taskId: newTaskId })).body);
		const comments = JSON.parse(listed.result.content[0].text).items;
		expect(comments).toHaveLength(1);
		expect(comments[0].author).toEqual({ kind: 'persona', id: 'p-1', name: 'Reviewer', owner: { id: '7', name: 'Yurij' } });
	});

	it('refuses update_task when a workspaceId is passed, even a matching one', async () => {
		await enableLocalPersona(ctx, 'p-1', ['tasks:write']);
		const res = await call('update_task', { taskId, title: 'Renamed', workspaceId: ctx.workspace.id });
		const result = JSON.parse(res.body).result;
		expect(result.isError).toBe(true);
		expect(result.content[0].text).toMatch(/move a task to another workspace/);
	});

	it('refuses tmgr_request against a route outside the persona whitelist', async () => {
		await enableLocalPersona(ctx, 'p-1', ['tasks:read']);
		const res = await call('tmgr_request', { method: 'GET', path: '/api/tasks/settings' });
		const result = JSON.parse(res.body).result;
		expect(result.isError).toBe(true);
	});

	it('refuses tmgr_request against local/* even when it looks like a normal path', async () => {
		await enableLocalPersona(ctx, 'p-1', ['tasks:read']);
		const res = await call('tmgr_request', { method: 'GET', path: '/api/local/whoami' });
		const result = JSON.parse(res.body).result;
		expect(result.isError).toBe(true);
		expect(result.content[0].text).toMatch(/local\/\*/);
	});

	it('answers whoami with the same snake_case shape as the cloud MCP, not camelized', async () => {
		await enableLocalPersona(ctx, 'p-1', ['tasks:read']);
		const res = JSON.parse((await call('whoami')).body).result;
		const whoami = JSON.parse(res.content[0].text);
		expect(whoami).toEqual({
			user_id: 7,
			persona: {
				id: 'p-1',
				name: 'Reviewer',
				description: 'Reviews PRs',
				owner: { id: 7, name: 'Yurij' },
				prompt_version: 2,
				workspace: { id: ctx.workspace.id, code: 'local-personal', permissions: ['tasks:read'] },
				skills: [],
			},
		});
	});

	it('refuses every tool call, including identity ones, once the persona is disabled locally', async () => {
		await enableLocalPersona(ctx, 'p-1', ['tasks:read']);
		await disableLocalPersona(ctx, 'p-1');
		const res = JSON.parse((await call('get_persona')).body).result;
		expect(res.isError).toBe(true);
		expect(res.content[0].text).toBe('Persona is disabled in this workspace');

		const listed = JSON.parse((await rpc({ jsonrpc: '2.0', id: 1, method: 'tools/list' })).body).result.tools;
		expect(listed.map((t: any) => t.name)).not.toContain('get_task');
	});

	it('parses agent-work commits as "<sha> <subject>" pairs, like the cloud tool', async () => {
		await enableLocalPersona(ctx, 'p-1', ['agent_work:write', 'agent_work:read']);
		const started = JSON.parse((await call('start_agent_work', { taskId, agent: 'claude-code' })).body);
		const runId = JSON.parse(started.result.content[0].text).id;
		await call('finish_agent_work', {
			runId,
			status: 'succeeded',
			commits: ['abc123 fix the thing', 'def456'],
		});
		const overview = JSON.parse((await call('list_agent_work', { taskId })).body);
		const runs = JSON.parse(overview.result.content[0].text).runs;
		expect(runs[0].commits).toEqual([
			{ sha: 'abc123', message: 'fix the thing' },
			{ sha: 'def456', message: null },
		]);
	});

	it('answers agent-work tools with the skip-reporting error while the toggle is off', async () => {
		await enableLocalPersona(ctx, 'p-1', ['agent_work:write', 'agent_work:read']);
		await dispatchLocal(router, { ...ctx, actor: undefined }, 'PUT', 'workspaces/-1/feature-toggles', {
			features: { agent_work: false },
		});
		for (const [name, args] of [
			['start_agent_work', { taskId, agent: 'claude-code' }],
			['update_agent_work', { runId: 1, summary: 'x' }],
			['finish_agent_work', { runId: 1, status: 'succeeded' }],
			['list_agent_work', { taskId }],
		] as const) {
			const result = JSON.parse((await call(name, args)).body).result;
			expect(result.isError).toBe(true);
			expect(result.content[0].text).toBe(
				'Agent work is disabled in workspace local-personal — skip agent-work reporting',
			);
		}
	});

	it('returns list_statuses as a plain array, matching the cloud tool', async () => {
		await enableLocalPersona(ctx, 'p-1', ['statuses:read']);
		const res = JSON.parse((await call('list_statuses')).body).result;
		const statuses = JSON.parse(res.content[0].text);
		expect(Array.isArray(statuses)).toBe(true);
		expect(statuses[0]).toHaveProperty('name');
	});

	it('answers get_persona_skill with a tool error since local workspaces have no skills yet', async () => {
		await enableLocalPersona(ctx, 'p-1', []);
		const res = JSON.parse((await call('get_persona_skill', { slug: 'anything' })).body).result;
		expect(res.isError).toBe(true);
		expect(res.content[0].text).toBe('No skills in local workspaces yet');
	});
});
