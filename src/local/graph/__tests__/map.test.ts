import { memoryDb, nodeSqliteAvailable } from '../../__tests__/nodeDb';
import { createLocalApi } from '../../api';
import { dispatchLocal } from '../../dispatch';
import { migrate } from '../../schema';
import type { BatchStatement, LocalContext } from '../../types';

const describeSqlite = nodeSqliteAvailable ? describe : describe.skip;

describeSqlite('local graph map on SQLite', () => {
	let ctx: LocalContext;
	let seq = 0;
	const api = createLocalApi();
	const day = (n: number) =>
		`2026-09-${String(n).padStart(2, '0')}T10:00:00.000Z`;

	const run = (sql: string, params: (string | number | null)[] = []) =>
		ctx.db.execute(sql, params);
	const status = async (name: string, type: string) =>
		(
			await run(
				`INSERT INTO statuses (name, type, sort_order, created_at, updated_at) VALUES (?, ?, 0, 'a', 'a')`,
				[name, type],
			)
		).lastInsertId!;
	const category = async (title: string, code: string) =>
		(
			await run(
				`INSERT INTO categories (title, slug, code, created_at, updated_at) VALUES (?, ?, ?, 'a', 'a')`,
				[title, title.toLowerCase(), code],
			)
		).lastInsertId!;
	const task = async (
		title: string,
		extra: {
			status?: number;
			category?: number;
			created?: string;
			updated?: string;
		} = {},
	) => {
		seq += 1;
		return (
			await run(
				`INSERT INTO tasks (title, status_id, project_category_id, category_tasks_sequence_id, created_at, updated_at)
				 VALUES (?, ?, ?, ?, ?, ?)`,
				[
					title,
					extra.status ?? null,
					extra.category ?? null,
					seq,
					extra.created ?? day(1),
					extra.updated ?? day(2),
				],
			)
		).lastInsertId!;
	};
	const page = async (title: string, updated = day(2)) =>
		(
			await run(
				`INSERT INTO pages (title, slug, type, author_id, author_kind, author_ref, updated_by_id, updated_by_kind, updated_by_ref, created_at, updated_at)
				 VALUES (?, ?, 'plain', 1, 'user', '1', 1, 'user', '1', ?, ?)`,
				[title, title.toLowerCase().replace(/\s+/g, '-'), day(1), updated],
			)
		).lastInsertId!;
	const relate = (a: number, b: number, typeName: string) =>
		run(
			`INSERT INTO task_relations (task_id, related_task_id, relation_type_id, created_at)
			 VALUES (?, ?, (SELECT id FROM task_relation_types WHERE name = ?), 'a')`,
			[a, b, typeName],
		);
	const link = (pageId: number, kind: string, targetId: number) =>
		run(
			`INSERT INTO page_links (page_id, target_kind, target_id) VALUES (?, ?, ?)`,
			[pageId, kind, targetId],
		);
	const ok = async (url: string) => {
		const res = await dispatchLocal(api, ctx, 'GET', url);
		if (!res || res.status >= 400)
			throw new Error(`${url} -> ${res?.status} ${JSON.stringify(res?.data)}`);
		return res.data.data;
	};
	const ids = (list: { id: string }[]) => list.map((n) => n.id);

	beforeEach(async () => {
		seq = 0;
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
			user: { id: 7, name: 'Me', email: 'me@example.com' },
			now: () => new Date('2026-10-01T10:00:00.000Z'),
			files: {
				url: (key) => `tmgrfile://localhost/${key}`,
				read: async () => new Blob(['x']),
				remove: async () => {},
			},
		};
		await migrate(ctx.db, '2026-10-01T10:00:00.000Z');
	});

	it('returns nodes with degree weight, meta, edges and categories', async () => {
		const cat = await category('Alpha', 'AL');
		const a = await task('A', { category: cat });
		const b = await task('B');
		const p = await page('P');
		await relate(a, b, 'blocks');
		await link(p, 'task', a);
		const out = await ok('graph/map');
		expect(out.total).toBe(3);
		expect(out.truncated).toBe(false);
		const node = out.nodes.find((n: any) => n.id === `task:${a}`);
		expect(node).toMatchObject({
			hop: 0,
			weight: 2,
			category_id: cat,
			category: 'Alpha',
			key: 'AL-1',
		});
		expect(node.meta).toMatchObject({
			created_at: day(1),
			updated_at: day(2),
			degree: 2,
			blocks: 1,
		});
		expect(out.nodes.find((n: any) => n.id === `page:${p}`)).toMatchObject({
			category_id: null,
			category: null,
			weight: 1,
		});
		expect(out.edges.map((e: any) => e.type).sort()).toEqual([
			'blocks',
			'linked_page',
		]);
		expect(out.edges.every((e: any) => e.why === '')).toBe(true);
		expect(out.categories).toEqual([
			{ id: null, title: 'Uncategorized', code: null, count: 1 },
			{ id: cat, title: 'Alpha', code: 'AL', count: 1 },
		]);
	});

	it('filters by the created/updated window and ignores deleted rows', async () => {
		const inside = await task('In', {
			created: '2026-09-10T00:00:00.000Z',
			updated: '2026-09-12T00:00:00.000Z',
		});
		await task('Too late', {
			created: '2026-09-20T00:00:00.000Z',
			updated: '2026-09-21T00:00:00.000Z',
		});
		await task('Too old', {
			created: '2026-08-01T00:00:00.000Z',
			updated: '2026-08-02T00:00:00.000Z',
		});
		const gone = await task('Gone', {
			created: '2026-09-10T00:00:00.000Z',
			updated: '2026-09-12T00:00:00.000Z',
		});
		await run(`UPDATE tasks SET deleted_at = 'x' WHERE id = ?`, [gone]);
		const edgeDay = await task('Edge', {
			created: '2026-09-15T08:00:00.000Z',
			updated: '2026-09-15T08:00:00.000Z',
		});
		const out = await ok('graph/map?from=2026-09-05&to=2026-09-15');
		expect(ids(out.nodes).sort()).toEqual(
			[`task:${inside}`, `task:${edgeDay}`].sort(),
		);
		expect((await ok('graph/map')).total).toBe(4);
		const res = await dispatchLocal(api, ctx, 'GET', 'graph/map?from=bad');
		expect(res!.status).toBe(422);
	});

	it('drops pages when the toggle is off by returning tasks only through the service', async () => {
		const { map } = await import('../map');
		await task('T');
		await page('P');
		const off = await map(ctx, {}, { pagesEnabled: false });
		expect(off.nodes.map((n) => n.type)).toEqual(['task']);
	});

	it('keeps the highest degree nodes over the limit and reports the total', async () => {
		const hub = await task('Hub');
		const spokes: number[] = [];
		for (let i = 0; i < 3; i++) {
			const t = await task(`S${i}`);
			await relate(hub, t, 'relates to');
			spokes.push(t);
		}
		await task('Loose');
		const out = await ok('graph/map?limit=2');
		expect(out.truncated).toBe(true);
		expect(out.total).toBe(5);
		expect(out.nodes).toHaveLength(2);
		expect(out.nodes[0].id).toBe(`task:${hub}`);
		expect(out.nodes[0].weight).toBe(1);
		expect(out.edges).toHaveLength(1);
	});

	it('computes hubs, open bottlenecks, bridges and orphans', async () => {
		const done = await status('Done', 'completed');
		const open = await status('Doing', 'active');
		const c1 = await category('One', 'ON');
		const c2 = await category('Two', 'TW');
		const blocker = await task('Blocker', { status: open, category: c1 });
		const x = await task('X', { status: open, category: c2 });
		const y = await task('Y', { category: c2 });
		const finished = await task('Finished', { status: done, category: c2 });
		await relate(blocker, x, 'blocks');
		await relate(y, blocker, 'depends on');
		await relate(blocker, finished, 'blocks');
		const looseOpen = await task('Loose', { status: open });
		await task('Loose done', { status: done });
		const lonePage = await page('Lone');
		const out = await ok('graph/map');
		expect(out.insights.hubs[0].node.id).toBe(`task:${blocker}`);
		expect(out.insights.hubs[0]).toMatchObject({ degree: 3, blocks: 2 });
		expect(out.insights.bottlenecks).toHaveLength(1);
		expect(out.insights.bottlenecks[0]).toMatchObject({ blocks: 2 });
		expect(out.insights.bottlenecks[0].node.id).toBe(`task:${blocker}`);
		expect(out.insights.bridges).toEqual([
			{ from_category_id: c1, to_category_id: c2, edges: 3 },
		]);
		expect(out.insights.orphans.total).toBe(2);
		expect(out.insights.orphans.ids.sort()).toEqual(
			[`page:${lonePage}`, `task:${looseOpen}`].sort(),
		);
	});

	it('orders bridge pairs with the null category first', async () => {
		const c1 = await category('One', 'ON');
		const a = await task('A', { category: c1 });
		const b = await task('B');
		await relate(a, b, 'relates to');
		const out = await ok('graph/map');
		expect(out.insights.bridges).toEqual([
			{ from_category_id: null, to_category_id: c1, edges: 1 },
		]);
	});

	it('maps 3000 tasks, 500 pages and 6000 links in under 1.5 s', async () => {
		const stmts: BatchStatement[] = [];
		for (let i = 1; i <= 3000; i++)
			stmts.push({
				sql: `INSERT INTO tasks (title, created_at, updated_at) VALUES (?, ?, ?)`,
				params: [`T${i}`, day(1), day(2)],
			});
		for (let i = 1; i <= 500; i++)
			stmts.push({
				sql: `INSERT INTO pages (title, slug, type, author_id, author_kind, author_ref, updated_by_id, updated_by_kind, updated_by_ref, created_at, updated_at)
				 VALUES (?, ?, 'plain', 1, 'user', '1', 1, 'user', '1', ?, ?)`,
				params: [`P${i}`, `p-${i}`, day(1), day(2)],
			});
		for (let i = 1; i <= 3000; i++)
			stmts.push({
				sql: `INSERT INTO task_relations (task_id, related_task_id, relation_type_id, created_at) VALUES (?, ?, 3, 'a')`,
				params: [i, (i % 3000) + 1],
			});
		for (let i = 0; i < 1500; i++)
			stmts.push({
				sql: `INSERT INTO page_links (page_id, target_kind, target_id) VALUES (?, 'task', ?)`,
				params: [(i % 500) + 1, i * 2 + 1],
			});
		for (let i = 0; i < 500; i++)
			stmts.push({
				sql: `INSERT INTO page_links (page_id, target_kind, target_id) VALUES (?, 'page', ?)`,
				params: [i + 1, ((i + 1) % 500) + 1],
			});
		for (let i = 0; i < 1000; i++)
			stmts.push({
				sql: `INSERT INTO task_page_mentions (task_id, page_id) VALUES (?, ?)`,
				params: [i * 3 + 1, (i % 500) + 1],
			});
		await ctx.db.batch(stmts);
		const started = performance.now();
		const out = await ok('graph/map?limit=5000');
		const elapsed = performance.now() - started;
		console.info(
			`graph/map 3000 tasks / 500 pages / 6000 links: ${Math.round(
				elapsed,
			)} ms`,
		);
		expect(out.nodes).toHaveLength(3500);
		expect(out.edges.length).toBe(6000);
		expect(elapsed).toBeLessThan(1500);
	});
});
