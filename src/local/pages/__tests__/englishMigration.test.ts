import { memoryDb, nodeSqliteAvailable } from '../../__tests__/nodeDb';
import { createLocalApi } from '../../api';
import { dispatchLocal } from '../../dispatch';
import { LATEST_SCHEMA, MIGRATIONS, migrate } from '../../schema';
import type { LocalActor, LocalContext } from '../../types';
import { refreshManagedSection } from '../service';

const describeSqlite = nodeSqliteAvailable ? describe : describe.skip;
const now = '2026-10-01T10:00:00.000Z';

const OLD_PERSON = [
	'## Кратко',
	'',
	'<!-- tmgr:section id="promises" owner="system" -->',
	'## Обещания',
	'- [T1](tmgr://task/1) — old',
	'<!-- /tmgr:section -->',
	'',
	'## Хронология',
	'',
	'- user entry about Обещания',
	'',
	'<!-- tmgr:section id="insights" owner="persona:p-1" -->',
	'## Инсайты',
	'<!-- /tmgr:section -->',
	'',
].join('\n');

const OLD_CONTEXT = [
	'## Как мы работаем',
	'',
	'<!-- tmgr:section id="agent-notes" owner="agents" -->',
	'## Заметки агентов',
	'note',
	'<!-- /tmgr:section -->',
	'',
].join('\n');

const insertPage = (
	db: ReturnType<typeof memoryDb>,
	id: number,
	title: string,
	type: string,
	body: string,
	version: number,
) =>
	db.execute(
		`INSERT INTO pages (id, title, slug, type, body, version, author_id, author_kind, author_ref, updated_by_id, updated_by_kind, updated_by_ref, created_at, updated_at)
		 VALUES (?, ?, ?, ?, ?, ?, 1, 'user', '1', 1, 'user', '1', ?, ?)`,
		[id, title, `slug-${id}`, type, body, version, now, now],
	);

const insertVersion = (
	db: ReturnType<typeof memoryDb>,
	id: number,
	version: number,
	body: string,
) =>
	db.execute(
		`INSERT INTO page_versions (page_id, version, title, body, author_id, author_kind, author_ref, created_at)
		 VALUES (?, ?, 't', ?, 1, 'user', '1', ?)`,
		[id, version, body, now],
	);

describeSqlite('English pages migration', () => {
	const seedOld = async () => {
		const db = memoryDb();
		await db.execute(`CREATE TABLE meta (key TEXT PRIMARY KEY, value TEXT)`);
		for (const migration of MIGRATIONS.filter((m) => m.version <= 11)) {
			for (const statement of migration.statements) await db.execute(statement);
		}
		await db.execute(
			`INSERT INTO meta (key, value) VALUES ('schema_version', '11')`,
		);
		await insertPage(db, 1, 'Иван', 'person', OLD_PERSON, 2);
		await insertVersion(db, 1, 1, OLD_PERSON);
		await insertVersion(db, 1, 2, OLD_PERSON);
		await insertPage(db, 2, 'Контекст воркспейса', 'context', OLD_CONTEXT, 1);
		await insertVersion(db, 2, 1, OLD_CONTEXT);
		await insertPage(db, 3, 'Моя заметка', 'context', OLD_CONTEXT, 1);
		await insertPage(db, 4, 'Контекст воркспейса', 'plain', 'x', 1);
		return db;
	};

	it('renames only managed headings, keeps user text, and is idempotent', async () => {
		const db = await seedOld();
		expect(await migrate(db, now)).toBe(LATEST_SCHEMA);
		const body = async (id: number) =>
			(await db.select<any>(`SELECT body FROM pages WHERE id = ?`, [id]))[0]
				.body as string;

		const person = await body(1);
		expect(person).toContain('" -->\n## Promises\n');
		expect(person).toContain('" -->\n## Insights\n');
		expect(person).toContain('## Кратко');
		expect(person).toContain('## Хронология');
		expect(person).toContain('- user entry about Обещания');
		expect(person).not.toContain('## Обещания');
		expect(await body(2)).toContain('" -->\n## Agent notes\nnote\n');

		const versions = await db.select<any>(
			`SELECT version, body FROM page_versions WHERE page_id = 1 ORDER BY version`,
		);
		expect(versions[0].body).toBe(OLD_PERSON);
		expect(versions[1].body).toBe(person);

		const titles = await db.select<any>(
			`SELECT id, title FROM pages ORDER BY id`,
		);
		expect(titles.map((row: any) => row.title)).toEqual([
			'Иван',
			'Workspace context',
			'Моя заметка',
			'Контекст воркспейса',
		]);

		const hits = await db.select<any>(
			`SELECT rowid FROM pages_fts WHERE pages_fts MATCH 'Promises'`,
		);
		expect(hits.map((row: any) => row.rowid)).toContain(1);

		const snapshot = JSON.stringify(
			await db.select(`SELECT * FROM pages ORDER BY id`),
		);
		for (const statement of MIGRATIONS[MIGRATIONS.length - 1].statements) {
			await db.execute(statement);
		}
		expect(
			JSON.stringify(await db.select(`SELECT * FROM pages ORDER BY id`)),
		).toBe(snapshot);
	});

	describe('real output layout', () => {
		const api = createLocalApi();
		let ctx: LocalContext;
		const data = async (
			method: string,
			url: string,
			body?: unknown,
			actor?: LocalActor,
		) => {
			const res = await dispatchLocal(
				api,
				{ ...ctx, actor },
				method,
				url,
				body,
			);
			if (!res || res.status >= 400)
				throw new Error(`${method} ${url} -> ${res?.status}`);
			return res.data.data;
		};

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
				user: { id: 7, name: 'Yurij', email: 'me@example.com' },
				now: () => new Date(now),
				files: {
					url: (key) => `tmgrfile://localhost/${key}`,
					read: async () => new Blob(['x']),
					remove: async () => {},
				},
			};
			await migrate(ctx.db, now);
		});

		it('puts the heading right after the marker in the template and after a promises rebuild', async () => {
			const person = await data('POST', 'pages', {
				title: 'Ann',
				type: 'person',
			});
			expect(person.body).toContain(
				'<!-- tmgr:section id="promises" owner="system" -->\n## Promises\n<!-- /tmgr:section -->',
			);
			await refreshManagedSection(ctx, person.id, 'promises', '- a', 'x');
			const rebuilt = await data('GET', `pages/${person.id}`);
			expect(rebuilt.body).toContain(
				'<!-- tmgr:section id="promises" owner="system" -->\n## Promises\n\n- a\n<!-- /tmgr:section -->',
			);
		});
	});
});
