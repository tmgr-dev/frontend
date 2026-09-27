import { createLocalApi } from '../api';
import { dispatchLocal } from '../dispatch';
import { taskExport, workspaceExport } from '../export';
import { LATEST_SCHEMA, MIGRATIONS, migrate, readSchemaVersion } from '../schema';
import type { LocalContext } from '../types';
import { memoryDb, nodeSqliteAvailable } from './nodeDb';

const describeSqlite = nodeSqliteAvailable ? describe : describe.skip;

describeSqlite('local workspace API on SQLite', () => {
	let ctx: LocalContext;
	const removed: string[] = [];
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
			files: {
				url: (key: string) => `tmgrfile://localhost/test/${key}`,
				read: async () => new Blob(['bytes']),
				remove: async (key: string) => {
					removed.push(key);
				},
			},
		};
		await migrate(ctx.db, clock.toISOString());
	});

	it('migrates a fresh database and seeds the four status types, archived last', async () => {
		expect(await readSchemaVersion(ctx.db)).toBe(LATEST_SCHEMA);
		const statuses = await data('GET', '/workspaces/statuses');
		expect(statuses.map((s: any) => s.type)).toEqual(['default', 'active', 'completed', 'archived']);
		expect(statuses[0].pivot.order).toBe(1);
	});

	it('serves a category page: subcategories, parent chain and its tasks across statuses', async () => {
		const root = await data('POST', 'project_categories', { title: 'Root', code: 'rt' });
		const child = await data('POST', 'project_categories', { title: 'Child', project_category_id: root.id });
		await data('POST', 'project_categories', { title: 'Grandchild', project_category_id: child.id });
		const statuses = await data('GET', 'workspaces/statuses');
		const archived = statuses.find((s: any) => s.type === 'archived');
		await data('POST', 'tasks', { title: 'Open', project_category_id: child.id });
		await data('POST', 'tasks', { title: 'Done', project_category_id: child.id, status_id: archived.id });
		await data('POST', 'tasks', { title: 'Elsewhere', project_category_id: root.id });

		const page = (await call('GET', `project_categories/children/${child.id}?page=1&per_page=10`)).data;
		expect(page.data.map((c: any) => c.title)).toEqual(['Grandchild']);
		expect(page.meta.total).toBe(1);

		const withParents = await data('GET', `project_categories/${child.id}/with/parents`);
		expect(withParents.title).toBe('Child');
		expect(withParents.parent_category.title).toBe('Root');
		expect(withParents.parent_category.parent_category).toBeNull();

		const all = (await call('GET', `tasks/?project_category_id=${child.id}&page=1&per_page=10`)).data;
		expect(all.data.map((t: any) => t.title).sort()).toEqual(['Done', 'Open']);
		const onlyArchived = await data('GET', `tasks?project_category_id=${child.id}&status_id=${archived.id}`);
		expect(onlyArchived.map((t: any) => t.title)).toEqual(['Done']);
	});

	it('keeps plugin storage per plugin within a quota', async () => {
		await call('PUT', 'plugins/tmgr.estimate/storage/last%20run', { value: '{"at":1}' });
		await call('PUT', 'plugins/tmgr.other/storage/last%20run', { value: '2' });
		expect(await data('GET', 'plugins/tmgr.estimate/storage/last%20run')).toEqual({ value: '{"at":1}' });
		expect(await data('GET', 'plugins/tmgr.estimate/storage/missing')).toEqual({ value: null });
		expect(await data('GET', 'plugins/tmgr.estimate/storage')).toEqual(['last run']);
		await call('DELETE', 'plugins/tmgr.estimate/storage/last%20run');
		expect(await data('GET', 'plugins/tmgr.estimate/storage')).toEqual([]);
		expect(await data('GET', 'plugins/tmgr.other/storage')).toEqual(['last run']);

		const big = JSON.stringify('x'.repeat(250_000));
		for (let i = 0; i < 20; i++) {
			expect((await call('PUT', `plugins/tmgr.big/storage/k${i}`, { value: big })).status).toBe(200);
		}
		expect((await call('PUT', 'plugins/tmgr.big/storage/k20', { value: big })).status).toBe(413);
		expect((await call('PUT', 'plugins/tmgr.big/storage/k0', { value: big })).status).toBe(200);
	});

	it('holds the plugin storage quota under concurrent writes and counts keys too', async () => {
		const big = JSON.stringify('x'.repeat(250_000));
		const results = await Promise.all(
			Array.from({ length: 25 }, (_, i) => call('PUT', `plugins/tmgr.race/storage/k${i}`, { value: big })),
		);
		expect(results.filter((r) => r.status === 200).length).toBeLessThanOrEqual(20);
		const [{ used }] = await ctx.db.select<{ used: number }>(
			`SELECT SUM(LENGTH(value)) AS used FROM plugin_kv WHERE plugin_id = 'tmgr.race'`,
		);
		expect(Number(used)).toBeLessThanOrEqual(5 * 1024 * 1024);

		for (let i = 0; i < 1000; i++) await call('PUT', `plugins/tmgr.keys/storage/${i}`, { value: '1' });
		expect((await call('PUT', 'plugins/tmgr.keys/storage/one-more', { value: '1' })).status).toBe(413);
		expect((await call('PUT', 'plugins/tmgr.keys/storage/0', { value: '2' })).status).toBe(200);
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
		expect(toggles['task.relations'].enabled).toBe(true);
		expect(await dispatchLocal(api, ctx, 'GET', 'tasks/1/nonexistent-route')).toBeNull();
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

	it('attaches files: presign a safe key, attach, list, sign a link, delete the bytes', async () => {
		const task = await data('POST', 'tasks', { title: 'With a screenshot' });
		const target = await data('POST', 'files/presign-upload', {
			file_name: 'Screen Shot 2026-09-26 at 10.00.png',
			content_type: 'image/png',
			size_bytes: 1200,
		});
		expect(target.key).toMatch(/^[0-9a-f-]+\/Screen-Shot-2026-09-26-at-10.00.png$/);
		expect(target.upload_url).toBe(`tmgrfile://localhost/test/${target.key}`);

		const file = await data('POST', `/tasks/${task.id}/files`, {
			file_name: 'Screen Shot.png',
			file_path: target.key,
			mime_type: 'image/png',
			size_bytes: 1200,
		});
		expect(file).toMatchObject({ task_id: task.id, name: 'Screen Shot.png', size: 1200, workspace_id: -42 });
		expect(await data('GET', `/tasks/${task.id}/files`)).toHaveLength(1);
		expect((await data('GET', `/files/${file.id}/signed-url`)).url).toBe(target.upload_url);
		const page = (await call('GET', 'workspaces/-42/files?images=true')).data;
		expect(page.data[0].task.title).toBe('With a screenshot');

		await call('DELETE', `/files/${file.id}`);
		expect(removed).toContain(target.key);
		expect(await data('GET', `/tasks/${task.id}/files`)).toEqual([]);
	});

	it('refuses oversized uploads and keys it did not hand out', async () => {
		expect((await call('POST', 'files/presign-upload', { file_name: 'a', size_bytes: 26 * 1024 * 1024 })).status).toBe(413);
		const task = await data('POST', 'tasks', { title: 'x' });
		expect((await call('POST', `tasks/${task.id}/files`, { file_path: '../../etc/passwd' })).status).toBe(422);
	});

	it('returns file bytes raw, without the envelope', async () => {
		const task = await data('POST', 'tasks', { title: 'x' });
		const target = await data('POST', 'files/presign-upload', { file_name: 'a.txt' });
		const file = await data('POST', `tasks/${task.id}/files`, { file_name: 'a.txt', file_path: target.key });
		const res = await call('GET', `files/${file.id}/content`);
		expect(res.data).toBeInstanceOf(Blob);
	});

	it('exports the workspace and a single task from the database', async () => {
		const category = await data('POST', 'project_categories', { title: 'Taskmgr', code: 'tm' });
		const task = await data('POST', 'tasks', { title: 'Export me', project_category_id: category.id });
		await data('POST', `tasks/${task.id}/comments`, { message: 'noted' });
		await data('POST', 'tasks', { title: 'Gone' }).then((t) => call('DELETE', `tasks/${t.id}`));

		const files = await workspaceExport(ctx.db, 'Personal', 'Yurij', '2026-09-26');
		expect(files.map((f) => f.path)).toEqual(['README.md', 'tasks/TM-1-export-me.md']);
		expect(files[1].content).toContain('**Yurij** · ');
		expect(files[1].content).toContain('noted');

		const single = await taskExport(ctx.db, task.id, 'Yurij');
		expect(single?.path).toBe('TM-1-export-me.md');
		expect(await taskExport(ctx.db, 9999, 'Yurij')).toBeNull();
	});

	it('migration 5 adds comment authorship, reactions and relations on top of a v4 db, and re-runs safely', async () => {
		const fresh = memoryDb();
		const now = clock.toISOString();
		for (const migration of MIGRATIONS.filter((m) => m.version <= 4)) {
			for (const statement of migration.statements) await fresh.execute(statement);
		}
		await fresh.execute(
			`INSERT INTO meta (key, value) VALUES ('schema_version', ?) ON CONFLICT(key) DO UPDATE SET value = excluded.value`,
			['4'],
		);
		expect(await readSchemaVersion(fresh)).toBe(4);

		expect(await migrate(fresh, now)).toBe(LATEST_SCHEMA);
		const columns = (await fresh.select<any>(`PRAGMA table_info(comments)`)).map((c: any) => c.name);
		expect(columns).toEqual(expect.arrayContaining(['author_kind', 'author_id', 'author_name']));

		// A migration cut short leaves one ALTER already applied; re-running must not throw.
		await fresh.execute(`UPDATE meta SET value = '4' WHERE key = 'schema_version'`);
		await expect(migrate(fresh, now)).resolves.toBe(LATEST_SCHEMA);
		expect(await readSchemaVersion(fresh)).toBe(LATEST_SCHEMA);
	});

	it('gives a plugin comment the author from ctx, ignoring any author fields in the body', async () => {
		const task = await data('POST', 'tasks', { title: 'Plugin task' });
		ctx = { ...ctx, actor: { kind: 'plugin', id: 'tmgr.estimate', name: 'Estimate' } };
		const comment = await data('POST', `tasks/${task.id}/comments`, {
			message: 'hi',
			author: { kind: 'user', id: '999', name: 'Spoofed' },
			author_kind: 'user',
			author_id: '999',
		});
		expect(comment.author).toEqual({ kind: 'plugin', id: 'tmgr.estimate', name: 'Estimate' });

		ctx = { ...ctx, actor: undefined };
		const own = await data('POST', `tasks/${task.id}/comments`, { message: 'from the app' });
		expect(own.author).toEqual({ kind: 'user', id: '7', name: 'Yurij' });
	});

	it('toggles a comment reaction per actor: on, off, and independently for a plugin', async () => {
		const task = await data('POST', 'tasks', { title: 'React' });
		const comment = await data('POST', `tasks/${task.id}/comments`, { message: 'hi' });

		const toggled = await data('POST', `comments/${comment.id}/reactions/toggle`, { emoji: '👍' });
		expect(toggled).toEqual({
			reactions: [{ emoji: '👍', count: 1, reacted: true, users: [{ id: 7, name: 'Yurij' }] }],
			task_id: task.id,
		});
		const untoggled = await data('POST', `comments/${comment.id}/reactions/toggle`, { emoji: '👍' });
		expect(untoggled.reactions).toEqual([]);

		ctx = { ...ctx, actor: { kind: 'plugin', id: 'tmgr.estimate', name: 'Estimate' } };
		const pluginReacted = await data('POST', `comments/${comment.id}/reactions/toggle`, { emoji: '🎉' });
		expect(pluginReacted.reactions).toEqual([{ emoji: '🎉', count: 1, reacted: true, users: [] }]);

		ctx = { ...ctx, actor: undefined };
		const list = await data('GET', `tasks/${task.id}/comments`);
		expect(list[0].reactions).toEqual([{ emoji: '🎉', count: 1, reacted: false, users: [] }]);

		expect((await call('POST', `comments/${comment.id}/reactions/toggle`, { emoji: '   ' })).status).toBe(422);
		expect((await call('POST', `comments/999999/reactions/toggle`, { emoji: '👍' })).status).toBe(404);
	});

	it('creates, lists and deletes task relations; refuses the same task and unknown tasks', async () => {
		const a = await data('POST', 'tasks', { title: 'A' });
		const b = await data('POST', 'tasks', { title: 'B' });
		const types = await data('GET', 'task-relation-types');
		expect(types.map((t: any) => t.name)).toEqual([
			'blocks',
			'is blocked by',
			'relates to',
			'duplicates',
			'is duplicated by',
			'depends on',
			'is dependency of',
		]);
		const blocks = types.find((t: any) => t.name === 'blocks');

		const created = await data('POST', `tasks/${a.id}/related-to/${b.id}/with/${blocks.id}`);
		expect(created).toMatchObject({ task_id: a.id, related_task_id: b.id, task_relation_type_id: blocks.id });

		const relations = await data('GET', `tasks/${a.id}/relations`);
		expect(relations).toEqual([
			{
				id: created.id,
				relation_type: { id: blocks.id, name: 'blocks' },
				related_task: {
					id: b.id,
					title: 'B',
					status_id: b.status_id,
					workspace_id: -42,
					project_category_id: null,
				},
			},
		]);
		expect((await data('GET', `tasks/${a.id}`)).relationTypeWithTask).toEqual(relations);

		expect((await call('POST', `tasks/${a.id}/related-to/${a.id}/with/${blocks.id}`)).status).toBe(422);
		expect((await call('POST', `tasks/${a.id}/related-to/999999/with/${blocks.id}`)).status).toBe(404);

		await call('DELETE', `tasks/${a.id}/related-to/${b.id}/with/${blocks.id}`);
		expect(await data('GET', `tasks/${a.id}/relations`)).toEqual([]);
	});
});
