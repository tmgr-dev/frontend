import { memoryDb, nodeSqliteAvailable } from '../../__tests__/nodeDb';
import { createLocalApi } from '../../api';
import { dispatchLocal } from '../../dispatch';
import { migrate } from '../../schema';
import type { LocalActor, LocalContext } from '../../types';
import { deletePluginPageData } from '../pageData';

const describeSqlite = nodeSqliteAvailable ? describe : describe.skip;

describeSqlite('plugin page data on SQLite', () => {
	let ctx: LocalContext;
	const api = createLocalApi();
	const clock = new Date('2026-10-01T10:00:00Z');
	const plugin = (id: string): LocalActor => ({
		kind: 'plugin',
		id,
		name: id,
		ownerId: id,
	});
	const call = async (
		method: string,
		url: string,
		body?: unknown,
		actor?: LocalActor,
	) => {
		const res = await dispatchLocal(api, { ...ctx, actor }, method, url, body);
		if (!res) throw new Error(`no local route for ${method} ${url}`);
		return res;
	};

	beforeEach(async () => {
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
			user: { id: 7, name: 'Me', email: 'me@example.com' },
			now: () => clock,
			files: {
				url: (key: string) => key,
				read: async () => new Blob([]),
				remove: async () => undefined,
			},
		};
		await migrate(ctx.db, clock.toISOString());
		await ctx.db.execute(
			`CREATE TABLE IF NOT EXISTS pages (id INTEGER PRIMARY KEY, deleted_at TEXT)`,
		);
		await ctx.db.execute(
			`INSERT INTO pages (id, deleted_at) VALUES (1, NULL), (2, NULL), (3, '2026-10-01')`,
		);
	});

	it('stores, reads, lists and deletes per-page values per plugin', async () => {
		const a = plugin('acme.dossier');
		await call(
			'PUT',
			'plugins/acme.dossier/pages/1/data/seen',
			{ value: '{"n":1}' },
			a,
		);
		await call(
			'PUT',
			'plugins/acme.dossier/pages/2/data/seen',
			{ value: '2' },
			a,
		);
		expect(
			(
				await call(
					'GET',
					'plugins/acme.dossier/pages/1/data/seen',
					undefined,
					a,
				)
			).data.data,
		).toEqual({
			value: '{"n":1}',
		});
		expect(
			(
				await call(
					'POST',
					'plugins/acme.dossier/page-data/query',
					{ page_ids: [1, 2, 9], key: 'seen' },
					a,
				)
			).data.data,
		).toEqual({ 1: '{"n":1}', 2: '2' });
		await call(
			'DELETE',
			'plugins/acme.dossier/pages/1/data/seen',
			undefined,
			a,
		);
		expect(
			(
				await call(
					'GET',
					'plugins/acme.dossier/pages/1/data/seen',
					undefined,
					a,
				)
			).data.data,
		).toEqual({
			value: null,
		});
	});

	it('refuses another plugin id and pages that are missing or trashed', async () => {
		const other = plugin('acme.other');
		expect(
			(
				await call(
					'GET',
					'plugins/acme.dossier/pages/1/data/k',
					undefined,
					other,
				)
			).status,
		).toBe(403);
		const a = plugin('acme.dossier');
		expect(
			(
				await call(
					'PUT',
					'plugins/acme.dossier/pages/3/data/k',
					{ value: '1' },
					a,
				)
			).status,
		).toBe(404);
		expect(
			(
				await call(
					'PUT',
					'plugins/acme.dossier/pages/99/data/k',
					{ value: '1' },
					a,
				)
			).status,
		).toBe(404);
	});

	it('caps a plugin at 1000 keys and removes data with deletePluginPageData', async () => {
		const a = plugin('acme.dossier');
		await ctx.db.execute(
			`INSERT INTO plugin_page_data (plugin_id, page_id, key, value, updated_at)
			 WITH RECURSIVE n(i) AS (SELECT 1 UNION ALL SELECT i + 1 FROM n WHERE i < 1000)
			 SELECT 'acme.dossier', 1, 'k' || i, '1', 'x' FROM n`,
		);
		expect(
			(
				await call(
					'PUT',
					'plugins/acme.dossier/pages/2/data/extra',
					{ value: '1' },
					a,
				)
			).status,
		).toBe(413);
		await deletePluginPageData(ctx.db, [1]);
		expect(
			(await ctx.db.select(`SELECT COUNT(*) AS c FROM plugin_page_data`))[0].c,
		).toBe(0);
	});
});
