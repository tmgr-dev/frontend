import { createLocalApi } from '../api';
import { dispatchLocal } from '../dispatch';
import { installLocalLiveUpdates } from '../liveUpdates';
import { handleMcpRequest } from '../mcp';
import { onTaskAssignment, type TaskAssignmentReport } from '../personaAssignees';
import { enableLocalPersona } from '../personas';
import { LATEST_SCHEMA, MIGRATIONS, migrate, readSchemaVersion } from '../schema';
import type { LocalContext } from '../types';
import { memoryDb, nodeSqliteAvailable } from './nodeDb';

const describeSqlite = nodeSqliteAvailable ? describe : describe.skip;

describeSqlite('persona assignees in local workspaces', () => {
	let ctx: LocalContext;
	const clock = new Date('2026-10-01T10:00:00Z');
	const router = createLocalApi();
	const deps = { personaPrompt: async () => null };
	const reports: TaskAssignmentReport[] = [];
	let off: () => void;

	const personaCtx = (uuid: string, name: string): LocalContext => ({
		...ctx,
		actor: { kind: 'persona', id: uuid, name },
	});
	const api = async (method: string, url: string, body?: unknown, as: LocalContext = ctx) => {
		const res = await dispatchLocal(router, as, method, url, body);
		return res!;
	};
	const createTask = async (title: string, extra: Record<string, unknown> = {}) =>
		(await api('POST', 'tasks', { title, ...extra })).data.data;
	const mcp = async (as: LocalContext, name: string, args: Record<string, unknown> = {}) => {
		const res = await handleMcpRequest(
			router,
			as,
			JSON.stringify({ jsonrpc: '2.0', id: 1, method: 'tools/call', params: { name, arguments: args } }),
			deps,
		);
		return JSON.parse(res.body).result;
	};

	beforeEach(async () => {
		reports.length = 0;
		off = onTaskAssignment((r) => reports.push(r));
		ctx = {
			db: memoryDb(),
			workspace: { id: -1, name: 'Personal', code: 'local-personal', schema_version: 0, created_at: '', path: '/tmp/x', database: '/tmp/x/workspace.db' },
			user: { id: 7, name: 'Yurij', email: 'me@example.com' },
			now: () => clock,
			files: { url: (key) => `tmgrfile://localhost/${key}`, read: async () => new Blob(['x']), remove: async () => {} },
		};
		await migrate(ctx.db, clock.toISOString());
		for (const [uuid, name, owner] of [
			['p-1', 'Reviewer', 7],
			['p-2', 'Writer', 7],
			['p-3', 'Stranger', 9],
			['p-4', 'Off', 7],
		] as const) {
			await ctx.db.execute(
				`INSERT INTO personas (uuid, owner_user_id, owner_name, name, description, avatar_file, synced_at, archived_at)
				 VALUES (?, ?, 'Owner', ?, 'desc', NULL, ?, NULL)`,
				[uuid, owner, name, clock.toISOString()],
			);
		}
		for (const uuid of ['p-1', 'p-2', 'p-3']) await enableLocalPersona(ctx, uuid, ['tasks:read', 'tasks:write']);
	});

	afterEach(() => off());

	it('adds the task_persona_assignees table in the latest migration', async () => {
		expect(MIGRATIONS[MIGRATIONS.length - 1].version).toBe(LATEST_SCHEMA);
		const fresh = memoryDb();
		await migrate(fresh, clock.toISOString());
		await fresh.execute(`DROP TABLE task_persona_assignees`);
		await fresh.execute(`UPDATE meta SET value = '8' WHERE key = 'schema_version'`);
		await migrate(fresh, clock.toISOString());
		expect(await readSchemaVersion(fresh)).toBe(LATEST_SCHEMA);
		const columns = await fresh.select<{ name: string }>(`PRAGMA table_info(task_persona_assignees)`);
		expect(columns.map((c) => c.name)).toEqual(['task_id', 'persona_uuid', 'owner_implied', 'created_at']);
	});

	it('lists only enabled, own, unarchived personas as assignable in the persona view shape', async () => {
		const res = await api('GET', 'workspaces/-1/assignable-personas');
		expect(res.data.data).toEqual([
			{ id: 'p-1', name: 'Reviewer', description: 'desc', avatar_url: null, owner: { id: 7, name: 'Owner' }, workspace_id: null },
			{ id: 'p-2', name: 'Writer', description: 'desc', avatar_url: null, owner: { id: 7, name: 'Owner' }, workspace_id: null },
		]);
	});

	it('assigns several personas, implies the owner and emits payloads in the persona shape', async () => {
		const task = await createTask('Queue me');
		expect(task.persona_assignees).toEqual([]);
		expect(task.assignees).toEqual([]);
		await api('POST', `tasks/${task.id}/personas/p-1`);
		const res = await api('POST', `tasks/${task.id}/personas/p-2`);
		expect(res.status).toBe(200);
		expect(res.data.data.persona_assignees.map((p: any) => p.id)).toEqual(['p-1', 'p-2']);
		expect(Object.keys(res.data.data.persona_assignees[0]).sort()).toEqual(
			['avatar_url', 'description', 'id', 'name', 'owner', 'workspace_id'],
		);
		expect(res.data.data.assignees).toHaveLength(1);
		expect(res.data.data.assignees[0].id).toBe(7);
		const rows = await ctx.db.select<any>(`SELECT owner_implied FROM task_persona_assignees`);
		expect(rows.every((r) => r.owner_implied === 1)).toBe(true);
	});

	it('drops the implied owner only when the last persona is removed', async () => {
		const task = await createTask('T');
		await api('POST', `tasks/${task.id}/personas/p-1`);
		await api('POST', `tasks/${task.id}/personas/p-2`);
		let res = await api('DELETE', `tasks/${task.id}/personas/p-1`);
		expect(res.data.data.persona_assignees.map((p: any) => p.id)).toEqual(['p-2']);
		expect(res.data.data.assignees).toHaveLength(1);
		res = await api('DELETE', `tasks/${task.id}/personas/Writer`);
		expect(res.data.data.persona_assignees).toEqual([]);
		expect(res.data.data.assignees).toEqual([]);
	});

	it('rejects a persona that is not assignable', async () => {
		const task = await createTask('T');
		expect((await api('POST', `tasks/${task.id}/personas/p-3`)).status).toBe(422);
		expect((await api('POST', `tasks/${task.id}/personas/p-4`)).status).toBe(422);
		expect((await api('POST', `tasks/99999/personas/p-1`)).status).toBe(404);
	});

	it('accepts persona_assignees on create, PUT and PATCH, leaving null or omitted untouched', async () => {
		const task = await createTask('T', { persona_assignees: ['p-1'] });
		expect(task.persona_assignees.map((p: any) => p.id)).toEqual(['p-1']);
		let res = await api('PATCH', `tasks/${task.id}`, { title: 'T2' });
		expect(res.data.data.persona_assignees).toHaveLength(1);
		res = await api('PUT', `tasks/${task.id}`, { title: 'T3', persona_assignees: null });
		expect(res.data.data.persona_assignees).toHaveLength(1);
		res = await api('PATCH', `tasks/${task.id}`, { persona_assignees: ['p-2'] });
		expect(res.data.data.persona_assignees.map((p: any) => p.id)).toEqual(['p-2']);
		res = await api('PUT', `tasks/${task.id}`, { title: 'T3', persona_assignees: [] });
		expect(res.data.data.persona_assignees).toEqual([]);
		expect(res.data.data.assignees).toEqual([]);
		expect((await api('PATCH', `tasks/${task.id}`, { persona_assignees: ['p-3'] })).status).toBe(422);
	});

	it('filters task lists by persona and my_personas', async () => {
		const a = await createTask('A', { persona_assignees: ['p-1'] });
		const b = await createTask('B', { persona_assignees: ['p-2'] });
		await createTask('C');
		const ids = async (q: string) => (await api('GET', `tasks?${q}`)).data.data.map((t: any) => t.id).sort();
		expect(await ids('persona=p-1')).toEqual([a.id]);
		expect(await ids('persona=p-2')).toEqual([b.id]);
		expect(await ids('my_personas=1')).toEqual([a.id, b.id].sort());
		const current = (await api('GET', 'tasks/current?persona=p-2')).data.data;
		expect(current.map((t: any) => t.id)).toEqual([b.id]);
		expect(current[0].persona_assignees[0].id).toBe('p-2');
	});

	it('forbids a persona token from changing persona assignees', async () => {
		const task = await createTask('T');
		const p = personaCtx('p-1', 'Reviewer');
		expect((await api('POST', `tasks/${task.id}/personas/p-1`, undefined, p)).status).toBe(403);
		expect((await api('DELETE', `tasks/${task.id}/personas/p-1`, undefined, p)).status).toBe(403);
		expect((await api('GET', 'workspaces/-1/assignable-personas', undefined, p)).status).toBe(403);
		expect((await api('PATCH', `tasks/${task.id}`, { persona_assignees: ['p-1'] }, p)).status).toBe(403);
		expect((await api('PATCH', `tasks/${task.id}`, { title: 'ok' }, p)).status).toBe(200);
	});

	it('emits assigned and unassigned events with the owner as user id on first and last persona', async () => {
		const task = await createTask('T');
		await api('POST', `tasks/${task.id}/personas/p-1`);
		await api('POST', `tasks/${task.id}/personas/p-2`);
		await api('DELETE', `tasks/${task.id}/personas/p-1`);
		await api('DELETE', `tasks/${task.id}/personas/p-2`);
		expect(reports.map((r) => [r.type, r.payload.user_ids])).toEqual([
			['task.assigned', [7]],
			['task.assigned', []],
			['task.unassigned', []],
			['task.unassigned', [7]],
		]);
		expect(reports[0].payload).toEqual({
			task_id: task.id,
			workspace_id: -1,
			user_ids: [7],
			personas: [{ uuid: 'p-1', name: 'Reviewer', owner_user_id: 7 }],
		});
	});

	it('refetches the task on a non-user assignment, and ignores the app\'s own', async () => {
		const fetchTask = jest.fn().mockResolvedValue({ id: 1 });
		const deliver = jest.fn();
		const stop = installLocalLiveUpdates({ deliver, fetchTask, invalidate: jest.fn() });
		const task = await createTask('T');
		await api('POST', `tasks/${task.id}/personas/p-1`);
		expect(fetchTask).not.toHaveBeenCalled();
		await api('POST', `tasks/${task.id}/personas/p-2`, undefined, { ...ctx, actor: { kind: 'plugin', id: 'x', name: 'x' } });
		await new Promise((r) => setTimeout(r, 0));
		expect(fetchTask).toHaveBeenCalledWith(-1, task.id);
		stop();
	});

	it('removes assignments when the task is deleted', async () => {
		const task = await createTask('T', { persona_assignees: ['p-1'] });
		await api('DELETE', `tasks/${task.id}`);
		expect(await ctx.db.select(`SELECT * FROM task_persona_assignees`)).toEqual([]);
	});

	describe('MCP', () => {
		it('gives a persona list_my_queue with only its own tasks, ordered and filterable', async () => {
			const a = await createTask('A', { persona_assignees: ['p-1'] });
			await createTask('B', { persona_assignees: ['p-2'] });
			const c = await createTask('C', { persona_assignees: ['p-1', 'p-2'] });
			const p = personaCtx('p-1', 'Reviewer');
			const result = await mcp(p, 'list_my_queue');
			const page = JSON.parse(result.content[0].text);
			expect(page.items.map((t: any) => t.id).sort()).toEqual([a.id, c.id].sort());
			expect(page.items[0].personaAssignees.length).toBeGreaterThan(0);
			const limited = JSON.parse((await mcp(p, 'list_my_queue', { limit: 1 })).content[0].text);
			expect(limited.items).toHaveLength(1);
			expect(limited.hasMore).toBe(true);
			const none = JSON.parse((await mcp(p, 'list_my_queue', { statusType: 'completed' })).content[0].text);
			expect(none.items).toEqual([]);
		});

		it('hides the user tools from a persona and refuses their calls', async () => {
			const p = personaCtx('p-1', 'Reviewer');
			const list = JSON.parse(
				(await handleMcpRequest(router, p, JSON.stringify({ jsonrpc: '2.0', id: 1, method: 'tools/list' }), deps)).body,
			).result.tools.map((t: any) => t.name);
			expect(list).toContain('list_my_queue');
			for (const name of ['assign_persona', 'unassign_persona', 'list_assignable_personas']) {
				expect(list).not.toContain(name);
				expect((await mcp(p, name, { taskId: 1, persona: 'p-1' })).isError).toBe(true);
			}
		});

		it('lets the user token list, assign and unassign by uuid or exact name', async () => {
			const task = await createTask('T');
			const listed = JSON.parse((await mcp(ctx, 'list_assignable_personas')).content[0].text);
			expect(listed.map((p: any) => p.id)).toEqual(['p-1', 'p-2']);
			const assigned = JSON.parse((await mcp(ctx, 'assign_persona', { taskId: task.id, persona: 'Reviewer' })).content[0].text);
			expect(assigned.personaAssignees.map((p: any) => p.id)).toEqual(['p-1']);
			await mcp(ctx, 'assign_persona', { taskId: task.id, persona: 'p-2' });
			const left = JSON.parse((await mcp(ctx, 'unassign_persona', { taskId: task.id, persona: 'p-1' })).content[0].text);
			expect(left.personaAssignees.map((p: any) => p.id)).toEqual(['p-2']);
			expect((await mcp(ctx, 'assign_persona', { taskId: task.id, persona: 'Stranger' })).isError).toBe(true);
			expect((await mcp(ctx, 'list_my_queue')).isError).toBe(true);
		});

		it('shows persona assignees in get_task for a persona', async () => {
			const task = await createTask('T', { persona_assignees: ['p-1'] });
			const res = await mcp(personaCtx('p-1', 'Reviewer'), 'get_task', { taskId: task.id });
			const got = JSON.parse(res.content[0].text);
			expect(got.personaAssignees[0].id).toBe('p-1');
			expect(got.assignees[0].email).toBeUndefined();
		});
	});
});
