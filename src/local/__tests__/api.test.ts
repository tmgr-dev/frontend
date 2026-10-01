import axios from 'axios';
import { createDataApi } from '../../pluginSystem/dataApi';
import { createLocalApi } from '../api';
import { dispatchLocal } from '../dispatch';
import { taskExport, workspaceExport } from '../export';
import { respond } from '../install';
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
	// Mirrors src/local/pinned.ts: the actor comes from the plugin's own headers, never the body.
	const pluginApi = (pluginId = 'tmgr.estimate', storageId = pluginId) =>
		createDataApi(
			axios.create({
				adapter: async (config) => {
					const headerPluginId = config.headers?.['X-TMGR-Plugin'];
					const headerStorageId = config.headers?.['X-TMGR-Plugin-Storage'];
					const actor = headerPluginId
						? {
								kind: 'plugin' as const,
								id: String(headerPluginId),
								name: String(headerPluginId),
								ownerId: headerStorageId
									? decodeURIComponent(String(headerStorageId))
									: String(headerPluginId),
						  }
						: undefined;
					const result = await dispatchLocal(
						api,
						{ ...ctx, actor },
						config.method ?? 'get',
						config.url ?? '',
						config.data,
						config.params,
					);
					if (!result) throw new Error(`no local route for ${config.method} ${config.url}`);
					return respond(config, result.status, result.data);
				},
			}),
			pluginId,
			storageId,
			pluginId,
			false,
			ctx.workspace.id,
		);

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

	it('measures the plugin storage quota in bytes, not characters, for multi-byte text', async () => {
		// 1.8M CJK characters are under the 5 MB character count but over 5 MB once encoded as UTF-8.
		const cjk = JSON.stringify('字'.repeat(1_800_000));
		expect((await call('PUT', 'plugins/tmgr.cjk/storage/k0', { value: cjk })).status).toBe(413);
		const [{ n }] = await ctx.db.select<{ n: number }>(
			`SELECT COUNT(*) AS n FROM plugin_kv WHERE plugin_id = 'tmgr.cjk'`,
		);
		expect(Number(n)).toBe(0);
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

	it('refuses a duplicate category code, case-insensitively, but allows saving a category unchanged', async () => {
		const a = await data('POST', 'project_categories', { title: 'Alpha', code: 'AB' });
		expect((await call('POST', 'project_categories', { title: 'Beta', code: 'ab' })).status).toBe(422);
		const b = await data('POST', 'project_categories', { title: 'Beta', code: 'CD' });

		expect((await call('PUT', `project_categories/${b.id}`, { title: 'Beta', code: 'ab' })).status).toBe(422);
		// Unchanged code (even by case) must not trip over the category's own row.
		expect((await call('PUT', `project_categories/${a.id}`, { title: 'Alpha v2', code: 'ab' })).status).toBe(200);
		expect((await call('PUT', `project_categories/${a.id}`, { title: 'Alpha v2' })).status).toBe(200);
	});

	it('derives a category code from its title when none is given, like the Java backend', async () => {
		const first = await data('POST', 'project_categories', { title: 'Разработка' });
		expect(first.code).toBe('RAZRABOTKA');
		const second = await data('POST', 'project_categories', { title: 'Разработка' });
		expect(second.code).toBe('RAZRABOTKA-1');
		const withExplicitCode = await data('POST', 'project_categories', { title: 'Other', code: 'OTHER' });
		expect(withExplicitCode.code).toBe('OTHER');
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
		expect(comment.author).toEqual({
			kind: 'plugin',
			id: 'tmgr.estimate',
			name: 'Estimate',
			owner: { id: String(ctx.user.id), name: ctx.user.name },
		});

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

	it('refuses to react to a comment whose task was deleted', async () => {
		const task = await data('POST', 'tasks', { title: 'Doomed' });
		const comment = await data('POST', `tasks/${task.id}/comments`, { message: 'hi' });
		await call('DELETE', `tasks/${task.id}`);
		expect((await call('POST', `comments/${comment.id}/reactions/toggle`, { emoji: '👍' })).status).toBe(404);
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

	it('deletes task relations and comment reactions when either task is deleted', async () => {
		const a = await data('POST', 'tasks', { title: 'A' });
		const b = await data('POST', 'tasks', { title: 'B' });
		const blocks = (await data('GET', 'task-relation-types')).find((t: any) => t.name === 'blocks');
		await call('POST', `tasks/${a.id}/related-to/${b.id}/with/${blocks.id}`);
		const comment = await data('POST', `tasks/${a.id}/comments`, { message: 'hi' });
		await call('POST', `comments/${comment.id}/reactions/toggle`, { emoji: '👍' });

		await call('DELETE', `tasks/${b.id}`);
		const [{ relations }] = await ctx.db.select<{ relations: number }>(
			`SELECT COUNT(*) AS relations FROM task_relations WHERE task_id = ? OR related_task_id = ?`,
			[a.id, a.id],
		);
		expect(Number(relations)).toBe(0);

		await call('DELETE', `tasks/${a.id}`);
		const [{ reactions }] = await ctx.db.select<{ reactions: number }>(
			`SELECT COUNT(*) AS reactions FROM comment_reactions WHERE comment_id = ?`,
			[comment.id],
		);
		expect(Number(reactions)).toBe(0);
	});

	it('migration 6 adds per-task plugin data and agent work runs on top of a v4 db, and re-runs safely', async () => {
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
		const tables = (await fresh.select<any>(`SELECT name FROM sqlite_master WHERE type = 'table'`)).map(
			(t: any) => t.name,
		);
		expect(tables).toEqual(
			expect.arrayContaining(['plugin_task_data', 'agent_work_runs', 'pages', 'page_versions', 'page_links', 'task_page_mentions']),
		);

		await fresh.execute(`UPDATE meta SET value = '4' WHERE key = 'schema_version'`);
		await expect(migrate(fresh, now)).resolves.toBe(LATEST_SCHEMA);
	});

	it('filters tasks by priority, status type, and updated/due dates including time, nulls never matching due', async () => {
		const t1 = await data('POST', 'tasks', {
			title: 'Low, due later',
			priority: 'low',
			expired_at: '2026-10-05T08:00:00Z',
		});
		const t2 = await data('POST', 'tasks', {
			title: 'High, due soon',
			priority: 'high',
			expired_at: '2026-10-01T08:00:00Z',
		});
		const t3 = await data('POST', 'tasks', { title: 'High, no due date', priority: 'high' });

		expect((await data('GET', 'tasks?priority=high')).map((t: any) => t.id).sort()).toEqual(
			[t2.id, t3.id].sort(),
		);
		expect(await data('GET', 'tasks?due_before=2026-10-02T00:00:00Z')).toEqual([
			expect.objectContaining({ id: t2.id }),
		]);
		expect(await data('GET', 'tasks?due_after=2026-10-02T00:00:00Z')).toEqual([
			expect.objectContaining({ id: t1.id }),
		]);
		expect(
			(await data('GET', 'tasks?status_type=default')).map((t: any) => t.id).sort(),
		).toEqual([t1.id, t2.id, t3.id].sort());
		expect(await data('GET', `tasks?updated_since=${clock.toISOString()}`)).toHaveLength(3);
		clock = new Date('2026-09-26T10:30:00Z');
		await data('PATCH', `tasks/${t2.id}`, { priority: 'urgent' });
		expect(
			(await data('GET', `tasks?updated_since=${clock.toISOString()}`)).map((t: any) => t.id),
		).toEqual([t2.id]);

		const byDue = await data('GET', 'tasks?sort=due&direction=asc');
		expect(byDue.map((t: any) => t.id)).toEqual([t2.id, t1.id, t3.id]);
	});

	it('gives a plugin a task key immediately on create, and the next number when a category is set on update', async () => {
		const category = await data('POST', 'project_categories', { title: 'Taskmgr', code: 'tm' });
		const other = await data('POST', 'project_categories', { title: 'Other', code: 'ot' });
		await data('POST', 'tasks', { title: 'other1', project_category_id: other.id });
		const api = pluginApi();

		const created = (await api.createTask({ title: 'One', project_category_id: category.id })) as any;
		expect(created.key).toBe('TM-1');

		const loose = await data('POST', 'tasks', { title: 'Loose' });
		const updated = (await api.updateTask(loose.id, { project_category_id: other.id })) as any;
		expect(updated.key).toBe('OT-2');

		const fetched = (await api.getTask(created.id)) as any;
		expect(fetched.key).toBe('TM-1');
		const listed = (await api.listTasks({
			statusId: null,
			categoryId: category.id,
			search: null,
			page: 1,
			perPage: 20,
		})) as any;
		expect(listed.items[0].key).toBe('TM-1');
	});

	it('round-trips priority and a markdown description byte-for-byte', async () => {
		const description =
			'  leading and trailing spaces  \r\n\r\nSome *markdown* with `code` and unicode: héllo 世界 🎉\r\n\r\n- item\r\n';
		const task = await data('POST', 'tasks', { title: 'Desc', description, priority: 'urgent' });
		expect(task.description).toBe(description);
		expect(task.priority).toBe('urgent');

		const fetched = await data('GET', `tasks/${task.id}`);
		expect(fetched.description).toBe(description);

		const updated = await data('PATCH', `tasks/${task.id}`, { priority: 'low' });
		expect(updated.priority).toBe('low');
	});

	it('stores and clears expired_at', async () => {
		const task = await data('POST', 'tasks', { title: 'Deadline', expired_at: '2026-10-01T12:00:00Z' });
		expect(task.expired_at).toBe('2026-10-01T12:00:00Z');
		const cleared = await data('PATCH', `tasks/${task.id}`, { expired_at: null });
		expect(cleared.expired_at).toBeNull();
	});

	it('normalises expired_at: keeps a date-only string as is, normalises a time to ISO UTC, rejects garbage', async () => {
		const dateOnly = await data('POST', 'tasks', { title: 'Date only', expired_at: '2026-10-01' });
		expect(dateOnly.expired_at).toBe('2026-10-01');

		const withOffset = await data('POST', 'tasks', {
			title: 'With offset',
			expired_at: '2026-10-01T15:00:00+03:00',
		});
		expect(withOffset.expired_at).toBe('2026-10-01T12:00:00Z');

		const clearedByEmptyString = await data('PATCH', `tasks/${dateOnly.id}`, { expired_at: '' });
		expect(clearedByEmptyString.expired_at).toBeNull();

		expect((await call('PATCH', `tasks/${dateOnly.id}`, { expired_at: 'not a date' })).status).toBe(422);
	});

	it('archiving a task via a plain field update stops its running timer', async () => {
		const statuses = await data('GET', 'workspaces/statuses');
		const archived = statuses.find((s: any) => s.type === 'archived');
		const task = await data('POST', 'tasks', { title: 'Busy' });
		await call('POST', `tasks/${task.id}/countdown`);
		clock = new Date('2026-09-26T10:05:00Z');

		const updated = await data('PATCH', `tasks/${task.id}`, { status_id: archived.id });

		expect(updated.status).toBe('archived');
		expect(updated.start_time).toBe(0);
		expect(updated.common_time).toBe(300);
	});

	it('stores, queries and deletes per-task plugin data, namespaced by plugin, removed when the task is deleted', async () => {
		const task = await data('POST', 'tasks', { title: 'With data' });
		const other = await data('POST', 'tasks', { title: 'Other' });

		expect(await data('GET', `plugins/tmgr.estimate/tasks/${task.id}/data/points`)).toEqual({ value: null });
		await call('PUT', `plugins/tmgr.estimate/tasks/${task.id}/data/points`, { value: '5' });
		await call('PUT', `plugins/tmgr.other/tasks/${task.id}/data/points`, { value: '"mine"' });
		expect(await data('GET', `plugins/tmgr.estimate/tasks/${task.id}/data/points`)).toEqual({ value: '5' });
		expect(await data('GET', `plugins/tmgr.other/tasks/${task.id}/data/points`)).toEqual({ value: '"mine"' });

		await call('PUT', `plugins/tmgr.estimate/tasks/${other.id}/data/points`, { value: '8' });
		expect(
			await data('POST', 'plugins/tmgr.estimate/task-data/query', {
				task_ids: [task.id, other.id],
				key: 'points',
			}),
		).toEqual({ [task.id]: '5', [other.id]: '8' });

		await call('DELETE', `plugins/tmgr.estimate/tasks/${task.id}/data/points`);
		expect(await data('GET', `plugins/tmgr.estimate/tasks/${task.id}/data/points`)).toEqual({ value: null });

		expect((await call('PUT', `plugins/tmgr.estimate/tasks/999999/data/points`, { value: '1' })).status).toBe(404);

		await call('DELETE', `tasks/${other.id}`);
		const [{ n }] = await ctx.db.select<{ n: number }>(
			`SELECT COUNT(*) AS n FROM plugin_task_data WHERE task_id = ?`,
			[other.id],
		);
		expect(Number(n)).toBe(0);
	});

	it('measures per-task plugin data quota in bytes too', async () => {
		const task = await data('POST', 'tasks', { title: 'CJK data' });
		const cjk = JSON.stringify('字'.repeat(1_800_000));
		expect(
			(await call('PUT', `plugins/tmgr.cjk/tasks/${task.id}/data/k0`, { value: cjk })).status,
		).toBe(413);
	});

	it('shares the plugin storage quota between kv and per-task data', async () => {
		const task = await data('POST', 'tasks', { title: 'Quota' });
		const big = JSON.stringify('x'.repeat(250_000));
		expect((await call('PUT', 'plugins/tmgr.big/storage/k0', { value: big })).status).toBe(200);
		for (let i = 1; i < 20; i++) {
			expect((await call('PUT', `plugins/tmgr.big/tasks/${task.id}/data/k${i}`, { value: big })).status).toBe(200);
		}
		expect((await call('PUT', `plugins/tmgr.big/tasks/${task.id}/data/k20`, { value: big })).status).toBe(413);
		expect((await call('PUT', 'plugins/tmgr.big/storage/k0', { value: big })).status).toBe(200);
	});

	it('runs the agent work lifecycle: list, start, update, finish, and time totals', async () => {
		const task = await data('POST', 'tasks', { title: 'Agent task' });

		expect(await data('GET', `tasks/${task.id}/agent-work`)).toEqual({
			runs: [],
			totals: { agent_seconds: 0, human_seconds: 0, human_timer_running: false },
		});

		const run = await data('POST', `tasks/${task.id}/agent-work`, {
			agent: 'Claude Code',
			model: 'opus',
			session_id: 'sess-1',
			branch: 'feat/x',
		});
		expect(run).toMatchObject({
			task_id: task.id,
			agent: 'claude-code',
			model: 'opus',
			session_id: 'sess-1',
			branch: 'feat/x',
			status: 'running',
			version: 1,
			commits: [],
			tests: null,
		});

		clock = new Date('2026-09-26T10:05:00Z');
		const overview = await data('GET', `tasks/${task.id}/agent-work`);
		expect(overview.runs[0].duration_seconds).toBe(300);
		expect(overview.totals.agent_seconds).toBe(300);

		const updated = await data('PATCH', `agent-work/${run.id}`, {
			branch: 'feat/y',
			summary: 'wip',
			commits: [{ sha: 'abc1234', message: 'x' }],
		});
		expect(updated.branch).toBe('feat/y');
		expect(updated.summary).toBe('wip');
		expect(updated.commits).toEqual([{ sha: 'abc1234', message: 'x' }]);
		expect(updated.version).toBe(2);

		clock = new Date('2026-09-26T10:10:00Z');
		const finished = await data('POST', `agent-work/${run.id}/finish`, {
			status: 'succeeded',
			summary: 'done',
			pr_url: 'https://example.com/pr/1',
		});
		expect(finished.status).toBe('succeeded');
		expect(finished.duration_seconds).toBe(600);
		expect(finished.version).toBe(3);
		expect(finished.branch).toBe('feat/y');
		expect(finished.pr_url).toBe('https://example.com/pr/1');

		expect((await data('GET', `tasks/${task.id}/agent-work`)).totals.agent_seconds).toBe(600);
		expect((await call('PATCH', `agent-work/${run.id}`, { summary: 'late' })).status).toBe(409);
		expect((await call('POST', `agent-work/${run.id}/finish`, { status: 'failed' })).status).toBe(409);
	});

	it('abandons a still-running run of the same agent and actor when a new one starts, ending at its last update', async () => {
		const task = await data('POST', 'tasks', { title: 'Two runs' });
		const first = await data('POST', `tasks/${task.id}/agent-work`, { agent: 'claude-code' });
		clock = new Date('2026-09-26T10:02:00Z');
		await data('PATCH', `agent-work/${first.id}`, { summary: 'progress' });
		clock = new Date('2026-09-26T10:07:00Z');
		const second = await data('POST', `tasks/${task.id}/agent-work`, { agent: 'claude-code' });

		const overview = await data('GET', `tasks/${task.id}/agent-work`);
		// Ends at the run's own last update, not "now" — it went silent, it was not just abandoned.
		expect(overview.runs.find((r: any) => r.id === first.id)).toMatchObject({
			status: 'abandoned',
			duration_seconds: 120,
		});
		expect(overview.runs.find((r: any) => r.id === second.id)).toMatchObject({ status: 'running' });

		await data('POST', `tasks/${task.id}/agent-work`, { agent: 'codex' });
		const stillRunning = (await data('GET', `tasks/${task.id}/agent-work`)).runs.find(
			(r: any) => r.id === second.id,
		);
		expect(stillRunning.status).toBe('running');
	});

	it('refuses to update or finish a run started by a different actor', async () => {
		const task = await data('POST', 'tasks', { title: 'Owned' });
		ctx = { ...ctx, actor: { kind: 'plugin', id: 'tmgr.agent', name: 'Agent' } };
		const run = await data('POST', `tasks/${task.id}/agent-work`, { agent: 'claude-code' });

		ctx = { ...ctx, actor: { kind: 'plugin', id: 'tmgr.other', name: 'Other' } };
		expect((await call('PATCH', `agent-work/${run.id}`, { summary: 'x' })).status).toBe(403);
		expect((await call('POST', `agent-work/${run.id}/finish`, { status: 'succeeded' })).status).toBe(403);

		ctx = { ...ctx, actor: undefined };
		expect((await call('PATCH', `agent-work/${run.id}`, { summary: 'x' })).status).toBe(403);
	});

	it('validates starting agent work: the task must exist, agent is required', async () => {
		expect((await call('POST', `tasks/999999/agent-work`, { agent: 'claude-code' })).status).toBe(404);
		const task = await data('POST', 'tasks', { title: 'x' });
		expect((await call('POST', `tasks/${task.id}/agent-work`, { agent: '  ' })).status).toBe(422);
	});

	it('includes the task timer in agent work totals', async () => {
		const task = await data('POST', 'tasks', { title: 'Timed agent' });
		await call('POST', `tasks/${task.id}/countdown`);
		clock = new Date('2026-09-26T10:03:00Z');

		const overview = await data('GET', `tasks/${task.id}/agent-work`);
		expect(overview.totals.human_seconds).toBe(180);
		expect(overview.totals.human_timer_running).toBe(true);
	});

	it('deletes agent work runs when the task is deleted', async () => {
		const task = await data('POST', 'tasks', { title: 'Gone soon' });
		await call('POST', `tasks/${task.id}/agent-work`, { agent: 'claude-code' });
		await call('DELETE', `tasks/${task.id}`);
		const [{ n }] = await ctx.db.select<{ n: number }>(
			`SELECT COUNT(*) AS n FROM agent_work_runs WHERE task_id = ?`,
			[task.id],
		);
		expect(Number(n)).toBe(0);
	});

	it('namespaces agent work started through a plugin under its own identity, exclusively', async () => {
		const task = await data('POST', 'tasks', { title: 'Plugin agent work' });
		const mine = pluginApi('tmgr.agent');
		const run = (await mine.startAgentWork(task.id, {
			agent: 'claude-code',
			model: null,
			sessionId: null,
			branch: null,
		})) as any;
		expect(run.agent).toBe('plugin:tmgr.agent/claude-code');

		const other = pluginApi('tmgr.other');
		await expect(other.updateAgentWork(run.id, { summary: 'hijack' })).rejects.toThrow();
		await expect(other.finishAgentWork(run.id, { status: 'succeeded' })).rejects.toThrow();

		const updated = (await mine.updateAgentWork(run.id, { summary: 'mine' })) as any;
		expect(updated.summary).toBe('mine');

		// A plugin's own second, unlabelled run does not collide with, or abandon, a different plugin's run.
		const bare = (await other.startAgentWork(task.id, {
			model: null,
			sessionId: null,
			branch: null,
		})) as any;
		expect(bare.agent).toBe('plugin:tmgr.other');
		const overview = await data('GET', `tasks/${task.id}/agent-work`);
		expect(overview.runs.find((r: any) => r.id === run.id).status).toBe('running');
	});

	it('rejects an agent name over 64 characters instead of silently truncating it', async () => {
		const task = await data('POST', 'tasks', { title: 'Long agent' });
		expect(
			(await call('POST', `tasks/${task.id}/agent-work`, { agent: 'a'.repeat(65) })).status,
		).toBe(422);
		expect(
			(await call('POST', `tasks/${task.id}/agent-work`, { agent: 'a'.repeat(64) })).status,
		).toBe(201);
	});

	it('identifies the agent work actor: persona with its owner, plugin, and the human user', async () => {
		const task = await data('POST', 'tasks', { title: 'Actors' });
		await ctx.db.execute(
			`INSERT INTO personas (uuid, owner_user_id, owner_name, name, description, avatar_file, synced_at, archived_at)
			 VALUES ('p-1', 7, 'Yurij', 'Reviewer', NULL, NULL, ?, NULL)`,
			[clock.toISOString()],
		);
		const { enableLocalPersona } = await import('../personas');
		await enableLocalPersona(ctx, 'p-1', ['agent_work:write']);
		const personaActor = { kind: 'persona' as const, id: 'p-1', name: 'Reviewer' };

		const personaResult = await dispatchLocal(
			api,
			{ ...ctx, actor: personaActor },
			'POST',
			`tasks/${task.id}/agent-work`,
			{ agent: 'reviewer-bot' },
		);
		expect(personaResult!.data.data.actor).toEqual({
			kind: 'persona',
			id: 'p-1',
			name: 'Reviewer',
			owner: { id: '7', name: 'Yurij' },
		});

		const pluginRun = (await pluginApi().startAgentWork(task.id, {
			agent: 'estimate',
			model: null,
			sessionId: null,
			branch: null,
		})) as any;
		expect(pluginRun.actor).toEqual({ kind: 'plugin', id: 'tmgr.estimate' });

		const userRun = await data('POST', `tasks/${task.id}/agent-work`, { agent: 'claude-code' });
		expect(userRun.actor).toEqual({ kind: 'user', id: String(ctx.user.id), name: ctx.user.name });

		const overview = await data('GET', `tasks/${task.id}/agent-work`);
		const personaRunInList = overview.runs.find((r: any) => r.agent === 'reviewer-bot');
		expect(personaRunInList.actor).toEqual({
			kind: 'persona',
			id: 'p-1',
			name: 'Reviewer',
			owner: { id: '7', name: 'Yurij' },
		});
	});

	it('isolates agent-work ownership by storage id, not the bare plugin id, when a different repo reuses it', async () => {
		const task = await data('POST', 'tasks', { title: 'Contested' });
		const authorA = pluginApi('tmgr.agent', 'tmgr.agent@github.com/authorA/repo');
		const authorB = pluginApi('tmgr.agent', 'tmgr.agent@github.com/authorB/repo');

		const run = (await authorA.startAgentWork(task.id, {
			agent: 'claude-code',
			model: null,
			sessionId: null,
			branch: null,
		})) as any;
		// The stored agent namespace is by bare plugin id, same for both repos.
		expect(run.agent).toBe('plugin:tmgr.agent/claude-code');

		await expect(authorB.updateAgentWork(run.id, { summary: 'hijack' })).rejects.toThrow();
		await expect(authorB.finishAgentWork(run.id, { status: 'succeeded' })).rejects.toThrow();
		const updated = (await authorA.updateAgentWork(run.id, { summary: 'mine' })) as any;
		expect(updated.summary).toBe('mine');

		// A same-named run from the other repo does not collide with, or abandon, author A's run.
		const other = (await authorB.startAgentWork(task.id, {
			agent: 'claude-code',
			model: null,
			sessionId: null,
			branch: null,
		})) as any;
		const overview = await data('GET', `tasks/${task.id}/agent-work`);
		expect(overview.runs.find((r: any) => r.id === run.id).status).toBe('running');
		expect(overview.runs.find((r: any) => r.id === other.id).status).toBe('running');

		// Reactions are owned by storage id too: both repos can react independently under the same plugin id.
		const comment = await data('POST', `tasks/${task.id}/comments`, { message: 'hi' });
		await authorA.reactToComment(comment.id, '👍');
		const afterA = (await authorA.listComments(task.id)) as any[];
		expect(afterA.find((c) => c.id === comment.id).reactions).toEqual([
			{ emoji: '👍', count: 1, reacted: true, users: [] },
		]);
		await authorB.reactToComment(comment.id, '👍');
		const afterB = (await authorB.listComments(task.id)) as any[];
		expect(afterB.find((c) => c.id === comment.id).reactions).toEqual([
			{ emoji: '👍', count: 2, reacted: true, users: [] },
		]);
		// Comment authorship still names the bare plugin id, regardless of which repo wrote it.
		const own = (await authorA.addComment(task.id, 'from A')) as any;
		expect(own.author).toMatchObject({ kind: 'plugin', id: 'tmgr.agent' });
	});

	it('taskData PUT and agent-work POST guard the task\'s existence inside the write itself, not a separate check', async () => {
		const task = await data('POST', 'tasks', { title: 'Deleted mid-flight' });
		await call('DELETE', `tasks/${task.id}`);

		const putRes = await call('PUT', `plugins/tmgr.race/tasks/${task.id}/data/k`, { value: '1' });
		expect(putRes.status).toBe(404);
		const [{ dataRows }] = await ctx.db.select<{ dataRows: number }>(
			`SELECT COUNT(*) AS dataRows FROM plugin_task_data WHERE task_id = ?`,
			[task.id],
		);
		expect(Number(dataRows)).toBe(0);

		const postRes = await call('POST', `tasks/${task.id}/agent-work`, { agent: 'claude-code' });
		expect(postRes.status).toBe(404);
		const [{ runRows }] = await ctx.db.select<{ runRows: number }>(
			`SELECT COUNT(*) AS runRows FROM agent_work_runs WHERE task_id = ?`,
			[task.id],
		);
		expect(Number(runRows)).toBe(0);
	});
});
