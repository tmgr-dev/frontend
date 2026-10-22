import { domainEvents, type DomainEvent } from '@/utils/domainEvents';
import { memoryDb, nodeSqliteAvailable } from '../../__tests__/nodeDb';
import { createLocalApi } from '../../api';
import { dispatchLocal } from '../../dispatch';
import { enableLocalPersona } from '../../personas';
import { migrate } from '../../schema';
import type { LocalActor, LocalContext } from '../../types';

const describeSqlite = nodeSqliteAvailable ? describe : describe.skip;

describeSqlite('purging archived pages on SQLite', () => {
	let ctx: LocalContext;
	let clock: Date;
	let remove: jest.Mock;
	const api = createLocalApi();
	const persona: LocalActor = { kind: 'persona', id: 'p-1', name: 'Analyst' };

	const call = async (
		method: string,
		url: string,
		body?: unknown,
		actor?: LocalActor,
	) => {
		clock = new Date(clock.getTime() + 1000);
		const res = await dispatchLocal(api, { ...ctx, actor }, method, url, body);
		if (!res) throw new Error(`no route ${method} ${url}`);
		return res;
	};
	const data = async (method: string, url: string, body?: unknown) => {
		const res = await call(method, url, body);
		if (res.status >= 400)
			throw new Error(
				`${method} ${url} -> ${res.status} ${JSON.stringify(res.data)}`,
			);
		return res.data.data;
	};
	const create = (title: string, extra: Record<string, unknown> = {}) =>
		data('POST', 'pages', { title, ...extra });
	const count = async (sql: string, params: number[]) =>
		(await ctx.db.select<{ n: number }>(sql, params))[0].n;
	const attach = (pageId: number, path: string) =>
		data('POST', `pages/${pageId}/files`, {
			file_name: path.split('/')[1],
			file_path: path,
		});
	const trashIds = async () =>
		(await data('GET', 'pages/trash')).map((p: any) => p.id);

	beforeEach(async () => {
		clock = new Date('2026-10-01T10:00:00.000Z');
		remove = jest.fn().mockResolvedValue(undefined);
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
			user: { id: 7, name: 'Yurij', email: 'me@example.com' },
			now: () => clock,
			files: {
				url: (key) => `tmgrfile://localhost/${key}`,
				read: async () => new Blob(['x']),
				remove,
			},
		};
		await migrate(ctx.db, clock.toISOString());
		await ctx.db.execute(
			`INSERT INTO personas (uuid, owner_user_id, owner_name, name, description, avatar_file, synced_at, archived_at)
			 VALUES ('p-1', 7, 'Yurij', 'Analyst', NULL, NULL, ?, NULL)`,
			[clock.toISOString()],
		);
		await enableLocalPersona(ctx, 'p-1', ['pages:read', 'pages:write']);
	});

	it('routes the new endpoints ahead of the page id patterns', () => {
		expect(api.match('POST', 'pages/trash/empty')?.pattern).toBe(
			'pages/trash/empty',
		);
		expect(api.match('DELETE', 'pages/12/permanent')?.pattern).toBe(
			'pages/:id/permanent',
		);
	});

	it('deletes the page archived together with its subtree and every row tied to them', async () => {
		const task = await data('POST', 'tasks', { title: 'T' });
		const keep = await create('Keep');
		const root = await create('Root', {
			body: `see [t](tmgr://task/${task.id}) and [k](tmgr://page/${keep.id})`,
		});
		const child = await create('Child', { parent_id: root.id });
		const grand = await create('Grand', { parent_id: child.id });
		const linker = await create('Linker', {
			body: `see [r](tmgr://page/${root.id})`,
		});
		await data('PATCH', `pages/${child.id}`, { version: 1, body: 'v2' });
		await attach(root.id, 'a-1/root.png');
		await attach(grand.id, 'b-2/grand.png');
		await attach(keep.id, 'c-3/keep.png');
		const ids = [root.id, child.id, grand.id];
		for (const id of [...ids, keep.id]) {
			await ctx.db.execute(
				`INSERT INTO task_page_mentions (task_id, page_id) VALUES (?, ?)`,
				[task.id, id],
			);
			await ctx.db.execute(
				`INSERT INTO plugin_page_data (plugin_id, page_id, key, value, updated_at) VALUES ('tmgr.x', ?, 'k', '1', '')`,
				[id],
			);
		}
		await data('DELETE', `pages/${root.id}`);

		expect(await data('DELETE', `pages/${root.id}/permanent`)).toEqual({
			deleted: 3,
			files: 2,
		});

		const marks = ids.map(() => '?').join(',');
		for (const table of [
			'pages WHERE id',
			'page_versions WHERE page_id',
			'page_links WHERE page_id',
			'task_page_mentions WHERE page_id',
			'files WHERE page_id',
			'plugin_page_data WHERE page_id',
		]) {
			expect(
				await count(`SELECT COUNT(*) AS n FROM ${table} IN (${marks})`, ids),
			).toBe(0);
		}
		expect(
			await count(
				`SELECT COUNT(*) AS n FROM page_links WHERE target_kind = 'page' AND target_id = ?`,
				[root.id],
			),
		).toBe(0);
		expect(
			await count(
				`SELECT COUNT(*) AS n FROM pages_fts WHERE pages_fts MATCH 'Grand'`,
				[],
			),
		).toBe(0);
		expect(remove.mock.calls.map(([path]: [string]) => path).sort()).toEqual([
			'a-1/root.png',
			'b-2/grand.png',
		]);
		const kept = await data('GET', `pages/${keep.id}`);
		expect(kept.files.map((f: any) => f.file_path)).toEqual(['c-3/keep.png']);
		expect((await data('GET', `pages/${linker.id}`)).body).toContain(
			`tmgr://page/${root.id}`,
		);
		expect(
			await count(
				`SELECT COUNT(*) AS n FROM task_page_mentions WHERE page_id = ?`,
				[keep.id],
			),
		).toBe(1);
		expect(await trashIds()).toEqual([]);
	});

	it('keeps a child archived separately as its own archive entry', async () => {
		const parent = await create('Parent');
		const child = await create('Child', { parent_id: parent.id });
		await data('DELETE', `pages/${child.id}`);
		await data('DELETE', `pages/${parent.id}`);

		expect(await data('DELETE', `pages/${parent.id}/permanent`)).toEqual({
			deleted: 1,
			files: 0,
		});

		expect(await trashIds()).toEqual([child.id]);
		const [row] = await ctx.db.select<{ parent_id: number | null }>(
			`SELECT parent_id FROM pages WHERE id = ?`,
			[child.id],
		);
		expect(row.parent_id).toBeNull();
		const restored = await data('POST', `pages/${child.id}/restore`);
		expect([restored.parent_id, restored.deleted_at]).toEqual([null, null]);
	});

	it('purges a page archived together with its parent and keeps the parent archived', async () => {
		const parent = await create('Parent');
		const child = await create('Child', { parent_id: parent.id });
		await data('DELETE', `pages/${parent.id}`);

		expect(await data('DELETE', `pages/${child.id}/permanent`)).toEqual({
			deleted: 1,
			files: 0,
		});
		expect(await trashIds()).toEqual([parent.id]);
	});

	it('refuses live and unknown pages and answers 404 for a second purge', async () => {
		const live = await create('Live');
		const res = await call('DELETE', `pages/${live.id}/permanent`);
		expect([res.status, res.data.error]).toEqual([422, 'page_not_deleted']);
		const unknown = await call('DELETE', 'pages/999/permanent');
		expect([unknown.status, unknown.data.error]).toEqual([
			404,
			'page_not_found',
		]);
		await data('DELETE', `pages/${live.id}`);
		await data('DELETE', `pages/${live.slug}/permanent`);
		const again = await call('DELETE', `pages/${live.id}/permanent`);
		expect([again.status, again.data.error]).toEqual([404, 'page_not_found']);
	});

	it('keeps a subpage restored out of the archive and refuses to restore a purged page', async () => {
		const parent = await create('Parent');
		const child = await create('Child', { parent_id: parent.id });
		await data('DELETE', `pages/${parent.id}`);
		await data('POST', `pages/${child.id}/restore`);

		expect(await data('DELETE', `pages/${parent.id}/permanent`)).toEqual({
			deleted: 1,
			files: 0,
		});
		const live = await data('GET', `pages/${child.id}`);
		expect([live.parent_id, live.deleted_at]).toEqual([null, null]);
		const restore = await call('POST', `pages/${parent.id}/restore`);
		expect(restore.status).toBe(404);
	});

	it('keeps the request successful when a file blob cannot be removed', async () => {
		const error = jest.spyOn(console, 'error').mockImplementation(() => {});
		remove.mockRejectedValueOnce(new Error('busy'));
		const page = await create('Doc');
		await attach(page.id, 'a-1/one.png');
		await attach(page.id, 'b-2/two.png');
		await data('DELETE', `pages/${page.id}`);

		expect(await data('DELETE', `pages/${page.id}/permanent`)).toEqual({
			deleted: 1,
			files: 2,
		});
		expect(remove).toHaveBeenCalledTimes(2);
		expect(error).toHaveBeenCalled();
		error.mockRestore();
	});

	it('empties the whole archive and answers zeros when it is empty', async () => {
		const a = await create('A');
		await create('A1', { parent_id: a.id });
		const b = await create('B');
		const b1 = await create('B1', { parent_id: b.id });
		await create('Live');
		await data('DELETE', `pages/${b1.id}`);
		await data('DELETE', `pages/${a.id}`);
		await data('DELETE', `pages/${b.id}`);

		expect(await data('POST', 'pages/trash/empty')).toEqual({
			purged: 3,
			deleted: 4,
			skipped: 0,
		});
		expect(await trashIds()).toEqual([]);
		expect((await data('GET', 'pages/tree')).map((p: any) => p.title)).toEqual([
			'Live',
		]);
		expect(await data('POST', 'pages/trash/empty')).toEqual({
			purged: 0,
			deleted: 0,
			skipped: 0,
		});
	});

	it('publishes page.purged once per purged root', async () => {
		const events: DomainEvent[] = [];
		const off = domainEvents.on((event) => {
			if (event.type === 'page.purged') events.push(event);
		});
		try {
			const a = await create('A');
			await create('A1', { parent_id: a.id });
			const b = await create('B');
			await data('DELETE', `pages/${a.id}`);
			await data('DELETE', `pages/${b.id}`);
			await data('DELETE', `pages/${a.id}/permanent`);
			await data('POST', 'pages/trash/empty');
			expect(events.map((e) => (e as any).pageId)).toEqual([a.id, b.id]);
			expect(events[0]).toMatchObject({
				workspaceId: -1,
				page: { id: a.id, slug: 'a', title: 'A', parent_id: null },
			});
		} finally {
			off();
		}
	});

	it('leaves purging to people', async () => {
		const page = await create('Doc');
		await data('DELETE', `pages/${page.id}`);
		for (const [method, url] of [
			['DELETE', `pages/${page.id}/permanent`],
			['POST', 'pages/trash/empty'],
		]) {
			expect((await call(method, url, {}, persona)).status).toBe(403);
		}
		expect(await trashIds()).toEqual([page.id]);
	});
});
