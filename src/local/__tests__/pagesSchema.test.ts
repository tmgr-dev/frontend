import { LATEST_SCHEMA, MIGRATIONS, migrate } from '../schema';
import type { LocalDb } from '../types';
import { memoryDb, nodeSqliteAvailable } from './nodeDb';

const describeSqlite = nodeSqliteAvailable ? describe : describe.skip;
const now = '2026-10-01T10:00:00.000Z';

const upTo = async (db: LocalDb, version: number) => {
	await db.execute(`CREATE TABLE meta (key TEXT PRIMARY KEY, value TEXT)`);
	for (const migration of MIGRATIONS.filter((m) => m.version <= version)) {
		for (const statement of migration.statements) await db.execute(statement);
	}
	await db.execute(`INSERT INTO meta (key, value) VALUES ('schema_version', ?)`, [String(version)]);
};

describeSqlite('pages schema (migration 9)', () => {
	it('creates the pages tables, the FTS index and its triggers', async () => {
		const db = memoryDb();
		expect(await migrate(db, now)).toBe(LATEST_SCHEMA);
		const names = (await db.select<any>(`SELECT name FROM sqlite_master`)).map((row) => row.name);
		expect(names).toEqual(
			expect.arrayContaining([
				'pages',
				'page_versions',
				'page_links',
				'task_page_mentions',
				'pages_fts',
				'pages_fts_ai',
				'pages_fts_ad',
				'pages_fts_au',
				'files_page_idx',
			]),
		);
	});

	it('keeps pages_fts in step with inserts, updates and deletes', async () => {
		const db = memoryDb();
		await migrate(db, now);
		const insert = (title: string, body: string) =>
			db.execute(
				`INSERT INTO pages (title, slug, body, author_id, author_kind, author_ref, updated_by_id, updated_by_kind, updated_by_ref, created_at, updated_at)
				 VALUES (?, ?, ?, 1, 'user', '1', 1, 'user', '1', ?, ?)`,
				[title, title.toLowerCase(), body, now, now],
			);
		const hits = async (term: string) =>
			(await db.select<any>(`SELECT rowid FROM pages_fts WHERE pages_fts MATCH ?`, [`"${term}"*`])).map(
				(row) => Number(row.rowid),
			);
		const { lastInsertId } = await insert('Alpha', 'first body with Привет');
		expect(await hits('prive')).toEqual([]);
		expect(await hits('привет')).toEqual([lastInsertId]);
		await db.execute(`UPDATE pages SET body = 'second text' WHERE id = ?`, [lastInsertId]);
		expect(await hits('привет')).toEqual([]);
		expect(await hits('second')).toEqual([lastInsertId]);
		await db.execute(`DELETE FROM pages WHERE id = ?`, [lastInsertId]);
		expect(await hits('second')).toEqual([]);
	});

	it('moves existing task files across the rebuild and survives a second run', async () => {
		const db = memoryDb();
		await upTo(db, 8);
		await db.execute(
			`INSERT INTO tasks (title, created_at, updated_at) VALUES ('T', ?, ?)`,
			[now, now],
		);
		await db.execute(
			`INSERT INTO files (task_id, name, file_path, mime_type, size, created_at) VALUES (1, 'a.png', 'k/a.png', 'image/png', 5, ?)`,
			[now],
		);
		expect(await migrate(db, now)).toBe(LATEST_SCHEMA);
		const rows = await db.select<any>(`SELECT id, task_id, page_id, name, file_path FROM files`);
		expect(rows).toEqual([{ id: 1, task_id: 1, page_id: null, name: 'a.png', file_path: 'k/a.png' }]);

		await db.execute(`UPDATE meta SET value = '8' WHERE key = 'schema_version'`);
		expect(await migrate(db, now)).toBe(LATEST_SCHEMA);
		expect(await db.select<any>(`SELECT task_id, file_path FROM files`)).toEqual([
			{ task_id: 1, file_path: 'k/a.png' },
		]);
		const next = await db.execute(
			`INSERT INTO files (task_id, name, file_path, created_at) VALUES (NULL, 'p', 'k/p.png', ?)`,
			[now],
		);
		expect(next.lastInsertId).toBe(2);
	});
});
