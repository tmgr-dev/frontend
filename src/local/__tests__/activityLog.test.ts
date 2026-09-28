import { createLocalApi } from '../api';
import { dispatchLocal } from '../dispatch';
import { enableLocalPersona } from '../personas';
import { migrate } from '../schema';
import type { LocalContext } from '../types';
import { memoryDb, nodeSqliteAvailable } from './nodeDb';

const describeSqlite = nodeSqliteAvailable ? describe : describe.skip;

describeSqlite('activity_log journal', () => {
	let ctx: LocalContext;
	let taskId: number;
	const clock = new Date('2026-09-28T10:00:00Z');
	const api = createLocalApi();
	const persona = { kind: 'persona' as const, id: 'p-1', name: 'Reviewer' };
	const plugin = { kind: 'plugin' as const, id: 'tmgr.estimate', name: 'tmgr.estimate' };

	const call = (method: string, url: string, body?: unknown, actor = ctx.actor) =>
		dispatchLocal(api, { ...ctx, actor }, method, url, body);

	const journalRows = () =>
		ctx.db.select<any>(`SELECT * FROM activity_log ORDER BY id ASC`);

	beforeEach(async () => {
		ctx = {
			db: memoryDb(),
			workspace: { id: -1, name: 'Personal', code: 'local-personal', schema_version: 0, created_at: '', path: '/tmp/x', database: '/tmp/x/workspace.db' },
			user: { id: 7, name: 'Yurij', email: 'me@example.com' },
			now: () => clock,
			files: { url: (key) => `tmgrfile://localhost/${key}`, read: async () => new Blob(['x']), remove: async () => {} },
		};
		await migrate(ctx.db, clock.toISOString());
		await ctx.db.execute(
			`INSERT INTO personas (uuid, owner_user_id, owner_name, name, description, avatar_file, synced_at, archived_at)
			 VALUES ('p-1', 7, 'Yurij', 'Reviewer', NULL, NULL, ?, NULL)`,
			[clock.toISOString()],
		);
		const created = await dispatchLocal(api, ctx, 'POST', 'tasks', { title: 'Do the thing' });
		taskId = created!.data.data.id;
	});

	it('journals a persona write with the matched route, entity and status', async () => {
		await enableLocalPersona(ctx, 'p-1', ['comments:write']);
		const res = await call('POST', `tasks/${taskId}/comments`, { message: 'Looks fine' }, persona);
		expect(res!.status).toBe(201);

		const rows = await journalRows();
		expect(rows).toHaveLength(1);
		expect(rows[0]).toMatchObject({
			actor_kind: 'persona',
			actor_id: 'p-1',
			actor_name: 'Reviewer',
			method: 'POST',
			route: 'tasks/:id(\\d+)/comments',
			entity: 'tasks',
			entity_id: String(taskId),
			status: 201,
		});
	});

	it('journals a plugin write', async () => {
		const res = await call('POST', `tasks/${taskId}/agent-work`, { agent: 'claude-code' }, plugin);
		expect(res!.status).toBe(201);

		const rows = await journalRows();
		expect(rows).toHaveLength(1);
		expect(rows[0]).toMatchObject({
			actor_kind: 'plugin',
			actor_id: 'tmgr.estimate',
			method: 'POST',
			route: 'tasks/:id(\\d+)/agent-work',
			entity: 'tasks',
			entity_id: String(taskId),
			status: 201,
		});
	});

	it('journals a refused persona GET (route not in the whitelist)', async () => {
		await enableLocalPersona(ctx, 'p-1', ['tasks:read']);
		const res = await call('GET', 'tasks/indexes', undefined, persona);
		expect(res!.status).toBe(403);

		const rows = await journalRows();
		expect(rows).toHaveLength(1);
		expect(rows[0]).toMatchObject({
			actor_kind: 'persona',
			method: 'GET',
			route: 'tasks/indexes',
			status: 403,
		});
	});

	it('writes no journal rows for the human user, on writes or refusals', async () => {
		await call('POST', `tasks/${taskId}/comments`, { message: 'from the user' }, undefined);
		await call('DELETE', `tasks/999999`, undefined, undefined);

		expect(await journalRows()).toHaveLength(0);
	});

	it('does not journal a plugin GET, only its writes', async () => {
		await call('GET', `tasks/${taskId}`, undefined, plugin);
		expect(await journalRows()).toHaveLength(0);
	});
});
