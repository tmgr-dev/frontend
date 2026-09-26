import { createLocalApi } from '../api';
import { dispatchLocal } from '../dispatch';
import { LATEST_SCHEMA, migrate, readSchemaVersion } from '../schema';
import type { LocalContext } from '../types';
import { memoryDb, nodeSqliteAvailable } from './nodeDb';

const describeSqlite = nodeSqliteAvailable ? describe : describe.skip;

describeSqlite('local workspace API on SQLite', () => {
	let ctx: LocalContext;
	let clock = new Date('2026-09-26T10:00:00Z');
	const api = createLocalApi();
	const call = async (method: string, url: string, body?: unknown) => {
		const res = await dispatchLocal(api, ctx, method, url, body);
		if (!res) throw new Error(`no local route for ${method} ${url}`);
		return res;
	};
	const data = async (method: string, url: string, body?: unknown) =>
		(await call(method, url, body)).data.data;

	beforeEach(async () => {
		clock = new Date('2026-09-26T10:00:00Z');
		ctx = {
			db: memoryDb(),
			workspace: {
				id: -42,
				name: 'Personal',
				code: 'local-personal',
				schema_version: 0,
				created_at: '',
				path: '/tmp/x',
				database: '/tmp/x/workspace.db',
			},
			user: { id: 7, name: 'Yurij', email: 'me@example.com' },
			now: () => clock,
		};
		await migrate(ctx.db, clock.toISOString());
	});

	it('migrates a fresh database and seeds the four status types, archived last', async () => {
		expect(await readSchemaVersion(ctx.db)).toBe(LATEST_SCHEMA);
		const statuses = await data('GET', '/workspaces/statuses');
		expect(statuses.map((s: any) => s.type)).toEqual(['default', 'active', 'completed', 'archived']);
		expect(statuses[0].pivot.order).toBe(1);
	});

	it('migrating twice is a no-op and does not duplicate statuses', async () => {
		await migrate(ctx.db, clock.toISOString());
		expect(await data('GET', 'workspaces/statuses')).toHaveLength(4);
	});

	it('refuses a database written by a newer app', async () => {
		await ctx.db.execute(`UPDATE meta SET value = '999' WHERE key = 'schema_version'`);
		await expect(migrate(ctx.db, '')).rejects.toThrow(/newer TMGR/);
	});

	it('creates a task in the default status and numbers tickets per category', async () => {
		const category = await data('POST', 'project_categories', { title: 'Taskmgr', code: 'tm' });
		const first = await data('POST', 'tasks', { title: 'One', project_category_id: category.id });
		const second = await data('POST', 'tasks', { title: 'Two', project_category_id: category.id });
		const loose = await data('POST', 'tasks', { title: 'No category' });

		expect(first.status).toBe('default');
		expect(first.category).toMatchObject({ id: category.id, code: 'TM', workspace_id: -42 });
		expect([first.category_tasks_sequence_id, second.category_tasks_sequence_id]).toEqual([1, 2]);
		expect(loose.category_tasks_sequence_id).toBeNull();
		expect(first.workspace_id).toBe(-42);
		expect((await call('POST', 'tasks', { title: '  ' })).status).toBe(422);
	});

	it('lists current tasks newest first, paginated and searchable, without archived ones', async () => {
		for (const title of ['Alpha', 'Beta', 'Gamma']) await data('POST', 'tasks', { title });
		const archived = await data('POST', 'tasks', { title: 'Old' });
		await call('PUT', `tasks/${archived.id}/done`);

		const page = (await call('GET', 'tasks/current?page=1&per_page=2')).data;
		expect(page.data.map((t: any) => t.title)).toEqual(['Gamma', 'Beta']);
		expect(page.meta).toMatchObject({ total: 3, last_page: 2, per_page: 2, current_page: 1 });
		const found = (await call('GET', 'tasks/current?search=alp')).data;
		expect(found.data.map((t: any) => t.title)).toEqual(['Alpha']);
		const done = (await call('GET', 'tasks/status/done?page=1')).data;
		expect(done.data.map((t: any) => t.title)).toEqual(['Old']);
	});

	it('orders a board column like the Java API: never-dragged first, then by order', async () => {
		const one = await data('POST', 'tasks', { title: 'one' });
		const two = await data('POST', 'tasks', { title: 'two' });
		const three = await data('POST', 'tasks', { title: 'three' });
		await call('PUT', 'tasks/update-orders', { tasks: [{ id: one.id, order: 2 }, { id: two.id, order: 1 }] });

		const column = await data(
			'GET',
			`tasks/status/${one.status_id}?all&order[column]=order&order[direction]=asc`,
		);
		expect(column.map((t: any) => t.title)).toEqual(['three', 'two', 'one']);
		expect(three.id).toBeGreaterThan(two.id);
	});

	it('runs the timer: start is idempotent, stop adds the elapsed time, runned lists running tasks', async () => {
		const task = await data('POST', 'tasks', { title: 'Timed' });
		await call('POST', `tasks/${task.id}/countdown`);
		clock = new Date('2026-09-26T10:00:30Z');
		await call('POST', `tasks/${task.id}/countdown`);
		expect((await data('GET', 'tasks/runned')).map((t: any) => t.id)).toEqual([task.id]);
		clock = new Date('2026-09-26T10:01:30Z');

		const stopped = await data('DELETE', `tasks/${task.id}/countdown`);

		expect(stopped.common_time).toBe(90);
		expect(stopped.start_time).toBe(0);
		expect(await data('GET', 'tasks/runned')).toEqual([]);
	});

	it('updates only the fields sent and keeps derived ones out of the database', async () => {
		const task = await data('POST', 'tasks', { title: 'Draft', checkpoints: [] });
		const updated = await data('PUT', `tasks/${task.id}`, {
			...task,
			title: 'Final',
			checkpoints: [{ description: 'x', checked: true }],
			category: { id: 999 },
			user: { id: 1 },
		});
		expect(updated.title).toBe('Final');
		expect(updated.checkpoints).toEqual([{ description: 'x', checked: true }]);
		expect(updated.category).toBeNull();
		const patched = await data('PATCH', `tasks/${task.id}`, { assignees: [1] });
		expect(patched.title).toBe('Final');
	});

	it('moving a running task to archived stops its timer', async () => {
		const task = await data('POST', 'tasks', { title: 'Busy' });
		await call('POST', `tasks/${task.id}/countdown`);
		clock = new Date('2026-09-26T10:05:00Z');
		const archived = await data('PUT', `tasks/${task.id}/done`);
		expect(archived.status).toBe('archived');
		expect(archived.start_time).toBe(0);
		expect(archived.common_time).toBe(300);
	});

	it('handles comments and soft-deletes tasks', async () => {
		const task = await data('POST', 'tasks', { title: 'Talk' });
		const comment = await data('POST', `/tasks/${task.id}/comments`, { message: 'hello' });
		expect(comment.user.name).toBe('Yurij');
		expect(await data('GET', `/tasks/${task.id}/comments/`)).toHaveLength(1);
		expect((await data('GET', `tasks/${task.id}`)).comments_count).toBe(1);
		await call('DELETE', `/tasks/${task.id}`);
		expect((await call('GET', `tasks/${task.id}`)).status).toBe(404);
	});

	it('refuses to delete a status that still has tasks', async () => {
		const task = await data('POST', 'tasks', { title: 'Stay' });
		expect((await call('DELETE', `statuses/${task.status_id}`)).status).toBe(409);
	});

	it('answers workspace odds and ends and leaves unknown routes unmatched', async () => {
		expect(await data('GET', 'workspaces/-42/members')).toEqual([
			expect.objectContaining({ id: 7, role: 'owner' }),
		]);
		const toggles = await data('GET', 'workspaces/-42/feature-toggles');
		expect(toggles.board.enabled).toBe(true);
		expect(toggles['task.files'].enabled).toBe(false);
		expect(await dispatchLocal(api, ctx, 'GET', 'tasks/1/files')).toBeNull();
	});

	it('gives a task the next ticket number of the category it moves to', async () => {
		const a = await data('POST', 'project_categories', { title: 'A', code: 'a' });
		const b = await data('POST', 'project_categories', { title: 'B', code: 'b' });
		await data('POST', 'tasks', { title: 'b1', project_category_id: b.id });
		const task = await data('POST', 'tasks', { title: 'moving', project_category_id: a.id });

		const moved = await data('PUT', `tasks/${task.id}`, { ...task, project_category_id: b.id });
		const same = await data('PUT', `tasks/${task.id}`, { ...moved, title: 'renamed' });
		const loose = await data('PUT', `tasks/${task.id}`, { ...same, project_category_id: null });

		expect(moved.category_tasks_sequence_id).toBe(2);
		expect(same.category_tasks_sequence_id).toBe(2);
		expect(loose.category_tasks_sequence_id).toBeNull();
	});

	it('restores missing default statuses when a previous migration stopped half-way', async () => {
		await ctx.db.execute('DELETE FROM statuses');
		await migrate(ctx.db, clock.toISOString());
		expect(await data('GET', 'workspaces/statuses')).toHaveLength(4);
	});

	it('does not mistake the task settings route for a status change', async () => {
		const task = await data('POST', 'tasks', { title: 'Set' });
		const res = await call('PUT', `tasks/${task.id}/settings`, [{ id: 1, value: 3 }]);
		expect(res.status).toBe(200);
		expect(res.data.data.status_id).toBe(task.status_id);
	});
});
