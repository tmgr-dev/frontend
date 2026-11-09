import { createLocalApi } from '../api';
import { dispatchLocal } from '../dispatch';
import { enableLocalPersona } from '../personas';
import {
	LATEST_SCHEMA,
	MIGRATIONS,
	migrate,
	readSchemaVersion,
} from '../schema';
import type { LocalContext } from '../types';
import { memoryDb, nodeSqliteAvailable } from './nodeDb';

const describeSqlite = nodeSqliteAvailable ? describe : describe.skip;

describeSqlite('task_status_history', () => {
	let ctx: LocalContext;
	let taskId: number;
	let backlog: number;
	let doing: number;
	let done: number;
	const clock = new Date('2026-11-07T10:00:00Z');
	const api = createLocalApi();

	const call = (method: string, url: string, body?: unknown) =>
		dispatchLocal(api, ctx, method, url, body);

	const history = () =>
		ctx.db.select<any>(`SELECT * FROM task_status_history ORDER BY id ASC`);

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
			user: { id: 7, name: 'Owner', email: 'me@example.com' },
			now: () => clock,
			files: {
				url: (key) => key,
				read: async () => new Blob(['x']),
				remove: async () => {},
			},
		};
		await migrate(ctx.db, clock.toISOString());
		const statuses = await ctx.db.select<any>(
			`SELECT id, name FROM statuses ORDER BY sort_order`,
		);
		[backlog, doing, done] = statuses.slice(0, 3).map((s: any) => s.id);
		const created = await call('POST', 'tasks', {
			title: 'Move me',
			status_id: backlog,
		});
		taskId = created!.data.data.id;
	});

	it('migration 15 creates the table and indexes', async () => {
		expect(LATEST_SCHEMA).toBeGreaterThanOrEqual(15);
		const columns = await ctx.db.select<any>(
			`PRAGMA table_info(task_status_history)`,
		);
		expect(columns.map((c: any) => c.name)).toEqual([
			'id',
			'task_id',
			'old_status_id',
			'new_status_id',
			'actor_kind',
			'created_at',
		]);
		const indexes = await ctx.db.select<any>(
			`PRAGMA index_list(task_status_history)`,
		);
		expect(indexes).toHaveLength(2);
	});

	it('creating a task records nothing', async () => {
		expect(await history()).toEqual([]);
	});

	it.each(['PUT', 'PATCH'])(
		'%s tasks/:id records one row when the status changes',
		async (method) => {
			await call(method, `tasks/${taskId}`, {
				title: 'Move me',
				status_id: doing,
			});
			expect(await history()).toEqual([
				expect.objectContaining({
					task_id: taskId,
					old_status_id: backlog,
					new_status_id: doing,
					actor_kind: 'user',
					created_at: clock.toISOString(),
				}),
			]);
		},
	);

	it('records nothing when the status stays the same or is not sent', async () => {
		await call('PATCH', `tasks/${taskId}`, { status_id: backlog });
		await call('PATCH', `tasks/${taskId}`, { title: 'Renamed' });
		expect(await history()).toEqual([]);
	});

	it('records one row for an edit that also changes the journaled title', async () => {
		await call('PUT', `tasks/${taskId}`, { title: 'New', status_id: done });
		expect(await history()).toHaveLength(1);
	});

	it('records nothing when the update fails on a missing task', async () => {
		const res = await call('PATCH', 'tasks/9999', { status_id: done });
		expect(res!.status).toBe(404);
		expect(await history()).toEqual([]);
	});

	it('the bulk status endpoint records only the tasks that actually move', async () => {
		const other = (await call('POST', 'tasks', {
			title: 'Already there',
			status_id: doing,
		}))!.data.data.id;
		await call('PUT', `statuses/${doing}/tasks`, {
			task_ids: [taskId, other],
		});
		expect(await history()).toEqual([
			expect.objectContaining({
				task_id: taskId,
				old_status_id: backlog,
				new_status_id: doing,
			}),
		]);
	});

	it('records the persona actor kind', async () => {
		const persona = { kind: 'persona' as const, id: 'p-1', name: 'Reviewer' };
		await ctx.db.execute(
			`INSERT INTO personas (uuid, owner_user_id, owner_name, name, description, avatar_file, synced_at, archived_at)
			 VALUES ('p-1', 7, 'Owner', 'Reviewer', NULL, NULL, ?, NULL)`,
			[clock.toISOString()],
		);
		await enableLocalPersona(ctx, 'p-1', ['tasks:write']);
		await dispatchLocal(
			api,
			{ ...ctx, actor: persona },
			'PATCH',
			`tasks/${taskId}`,
			{
				status_id: doing,
			},
		);
		expect((await history()).map((r: any) => r.actor_kind)).toEqual([
			'persona',
		]);
	});

	it('deleting the task removes its history', async () => {
		await call('PATCH', `tasks/${taskId}`, { status_id: doing });
		await ctx.db.execute(`PRAGMA foreign_keys = ON`);
		await ctx.db.execute(`DELETE FROM tasks WHERE id = ?`, [taskId]);
		expect(await history()).toEqual([]);
	});

	it('upgrades a v14 database without losing data and reruns idempotently', async () => {
		const db = memoryDb();
		for (const migration of MIGRATIONS.filter((m) => m.version <= 14))
			for (const statement of migration.statements) await db.execute(statement);
		await db.execute(
			`INSERT INTO meta (key, value) VALUES ('schema_version', '14')`,
		);
		await db.execute(
			`INSERT INTO statuses (id, name, type, color, sort_order, created_at, updated_at)
			 VALUES (1, 'Todo', 'default', '#000', 1, 'x', 'x')`,
		);
		await db.execute(
			`INSERT INTO tasks (id, title, status_id, settings, created_at, updated_at)
			 VALUES (1, 'Kept', 1, '[]', 'x', 'x')`,
		);
		await db.execute(
			`INSERT INTO comments (id, task_id, message, created_at, updated_at)
			 VALUES (1, 1, 'Note', 'x', 'x')`,
		);
		expect(await readSchemaVersion(db)).toBe(14);

		await migrate(db, clock.toISOString());
		for (const statement of MIGRATIONS.find((m) => m.version === 15)!
			.statements)
			await db.execute(statement);
		await migrate(db, clock.toISOString());

		expect(await readSchemaVersion(db)).toBe(15);
		expect(await db.select(`SELECT id, title, status_id FROM tasks`)).toEqual([
			{ id: 1, title: 'Kept', status_id: 1 },
		]);
		expect(await db.select(`SELECT message FROM comments`)).toEqual([
			{ message: 'Note' },
		]);
		expect(await db.select(`SELECT name FROM statuses ORDER BY id`)).toEqual([
			{ name: 'Todo' },
		]);
		expect(await db.select(`SELECT * FROM task_status_history`)).toEqual([]);
	});
});
