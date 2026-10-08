import { memoryDb, nodeSqliteAvailable } from '../../__tests__/nodeDb';
import { createLocalApi } from '../../api';
import { classify } from '../../classify';
import { dispatchLocal } from '../../dispatch';
import { migrate } from '../../schema';
import type { LocalActor, LocalContext } from '../../types';

const describeSqlite = nodeSqliteAvailable ? describe : describe.skip;

describeSqlite('local graph on SQLite', () => {
	let ctx: LocalContext;
	let seq = 0;
	let clock: Date;
	const api = createLocalApi();

	const run = (sql: string, params: (string | number | null)[] = []) =>
		ctx.db.execute(sql, params);
	const stamp = () => {
		seq += 1;
		return new Date(clock.getTime() + seq * 1000).toISOString();
	};

	const status = async (name: string, type: string) =>
		(
			await run(
				`INSERT INTO statuses (name, type, sort_order, created_at, updated_at) VALUES (?, ?, 0, ?, ?)`,
				[name, type, stamp(), stamp()],
			)
		).lastInsertId!;
	const category = async (title: string, code: string) =>
		(
			await run(
				`INSERT INTO categories (title, slug, code, created_at, updated_at) VALUES (?, ?, ?, ?, ?)`,
				[title, title.toLowerCase(), code, stamp(), stamp()],
			)
		).lastInsertId!;
	const task = async (
		title: string,
		extra: { status?: number; category?: number; seq?: number } = {},
	) =>
		(
			await run(
				`INSERT INTO tasks (title, status_id, project_category_id, category_tasks_sequence_id, created_at, updated_at)
				 VALUES (?, ?, ?, ?, ?, ?)`,
				[
					title,
					extra.status ?? null,
					extra.category ?? null,
					extra.seq ?? null,
					stamp(),
					stamp(),
				],
			)
		).lastInsertId!;
	const page = async (
		title: string,
		slug = title.toLowerCase().replace(/\s+/g, '-'),
	) =>
		(
			await run(
				`INSERT INTO pages (title, slug, type, author_id, author_kind, author_ref, updated_by_id, updated_by_kind, updated_by_ref, created_at, updated_at)
				 VALUES (?, ?, 'plain', 1, 'user', '1', 1, 'user', '1', ?, ?)`,
				[title, slug, stamp(), stamp()],
			)
		).lastInsertId!;
	const relate = (a: number, b: number, typeName: string) =>
		run(
			`INSERT INTO task_relations (task_id, related_task_id, relation_type_id, created_at)
			 VALUES (?, ?, (SELECT id FROM task_relation_types WHERE name = ?), ?)`,
			[a, b, typeName, stamp()],
		);
	const link = (pageId: number, kind: string, targetId: number) =>
		run(
			`INSERT INTO page_links (page_id, target_kind, target_id) VALUES (?, ?, ?)`,
			[pageId, kind, targetId],
		);
	const mention = (taskId: number, pageId: number) =>
		run(`INSERT INTO task_page_mentions (task_id, page_id) VALUES (?, ?)`, [
			taskId,
			pageId,
		]);
	const persona = async (uuid: string, name: string) => {
		await run(
			`INSERT INTO personas (uuid, owner_user_id, owner_name, name, synced_at) VALUES (?, 7, 'Me', ?, ?)`,
			[uuid, name, stamp()],
		);
		await run(
			`INSERT INTO workspace_personas (persona_uuid, permissions, enabled_at) VALUES (?, '[]', ?)`,
			[uuid, stamp()],
		);
	};
	const assign = (taskId: number, uuid: string) =>
		run(
			`INSERT INTO task_persona_assignees (task_id, persona_uuid, created_at) VALUES (?, ?, ?)`,
			[taskId, uuid, stamp()],
		);

	const get = async (url: string, actor?: LocalActor) => {
		const res = await dispatchLocal(api, { ...ctx, actor }, 'GET', url);
		if (!res) throw new Error(`no route ${url}`);
		return res;
	};
	const ok = async (url: string) => {
		const res = await get(url);
		if (res.status >= 400)
			throw new Error(`${url} -> ${res.status} ${JSON.stringify(res.data)}`);
		return res.data.data;
	};
	const ids = (list: { id: string }[]) => list.map((n) => n.id);

	beforeEach(async () => {
		seq = 0;
		clock = new Date('2026-10-01T10:00:00.000Z');
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
			now: () => clock,
			files: {
				url: (key) => `tmgrfile://localhost/${key}`,
				read: async () => new Blob(['x']),
				remove: async () => {},
			},
		};
		await migrate(ctx.db, clock.toISOString());
	});

	it('routes graph requests of a local workspace to the local adapter', () => {
		expect(classify('GET', 'graph/related?entity=task:1', true)).toBe('local');
		expect(classify('GET', 'graph/hubs', true)).toBe('local');
	});

	it('returns the node and edge shape with key, status, category and rel', async () => {
		const done = await status('In progress', 'active');
		const cat = await category('Current project', 'CP');
		const a = await task('Alpha', { status: done, category: cat, seq: 60 });
		const b = await task('Beta', { category: cat, seq: 61 });
		await relate(a, b, 'relates to');
		const out = await ok(`graph/related?entity=task:${a}`);
		expect(out.center).toBe(`task:${a}`);
		const center = out.nodes[0];
		expect(center).toEqual({
			id: `task:${a}`,
			type: 'task',
			ref_id: a,
			key: 'CP-60',
			title: 'Alpha',
			status: { name: 'In progress', type: 'active' },
			category_id: cat,
			category: 'Current project',
			hop: 0,
			weight: 5,
			rel: null,
			meta: {},
		});
		expect(out.nodes[1]).toMatchObject({
			id: `task:${b}`,
			key: 'CP-61',
			hop: 1,
			weight: 2,
			rel: 'relates to',
		});
		expect(out.edges).toEqual([
			{
				id: `relates_to:task:${a}:task:${b}`,
				from: `task:${a}`,
				to: `task:${b}`,
				type: 'relates_to',
				label: 'relates to',
				weight: 2,
				why: 'CP-60 relates to CP-61 (task relation)',
			},
		]);
		expect(out.truncated).toBe(false);
		expect(out.caps_hit).toEqual({});
	});

	it('accepts task keys and page slugs as entity', async () => {
		const cat = await category('Project', 'ABC');
		const t = await task('Keyed', { category: cat, seq: 12 });
		const p = await page('Design Notes', 'design-notes');
		await mention(t, p);
		expect((await ok('graph/related?entity=ABC-12')).center).toBe(`task:${t}`);
		expect((await ok('graph/related?entity=abc-12')).center).toBe(`task:${t}`);
		expect((await ok('graph/related?entity=page:design-notes')).center).toBe(
			`page:${p}`,
		);
	});

	it('answers 404 for a missing or deleted entity and 422 for a malformed one', async () => {
		const t = await task('Gone');
		await run(`UPDATE tasks SET deleted_at = ? WHERE id = ?`, [stamp(), t]);
		expect((await get(`graph/related?entity=task:${t}`)).status).toBe(404);
		expect((await get('graph/related?entity=task:999')).status).toBe(404);
		expect((await get('graph/related?entity=page:nope')).status).toBe(404);
		expect((await get('graph/related?entity=ZZ-1')).status).toBe(404);
		expect((await get('graph/related?entity=user:7')).status).toBe(422);
		expect((await get('graph/related')).status).toBe(422);
	});

	it('folds inverse relation names into one directed edge from both sides', async () => {
		const a = await task('A');
		const b = await task('B');
		const c = await task('C');
		const d = await task('D');
		await relate(a, b, 'blocks');
		await relate(c, a, 'is blocked by');
		await relate(a, d, 'is dependency of');
		const out = await ok(`graph/related?entity=task:${a}&depth=1`);
		const byId = Object.fromEntries(out.edges.map((e: any) => [e.id, e]));
		expect(byId[`blocks:task:${a}:task:${b}`]).toMatchObject({
			weight: 3,
			label: 'blocks',
		});
		expect(byId[`blocks:task:${a}:task:${c}`]).toBeDefined();
		expect(byId[`depends_on:task:${d}:task:${a}`]).toMatchObject({
			label: 'depends on',
		});
		expect(out.edges).toHaveLength(3);
		const fromB = await ok(`graph/related?entity=task:${b}&depth=1`);
		expect(fromB.edges.map((e: any) => e.id)).toEqual([
			`blocks:task:${a}:task:${b}`,
		]);
	});

	it('dedupes relates_to stored in both directions', async () => {
		const a = await task('A');
		const b = await task('B');
		await relate(a, b, 'relates to');
		await relate(b, a, 'relates to');
		const out = await ok(`graph/related?entity=task:${b}`);
		expect(out.edges).toHaveLength(1);
		expect(out.edges[0].id).toBe(`relates_to:task:${a}:task:${b}`);
	});

	it('walks page links, backlinks and task mentions', async () => {
		const t = await task('Task');
		const p1 = await page('Page One');
		const p2 = await page('Page Two');
		const p3 = await page('Page Three');
		await link(p1, 'task', t);
		await link(p1, 'page', p2);
		await link(p3, 'page', p1);
		await mention(t, p2);
		const out = await ok(`graph/related?entity=page:${p1}&depth=1`);
		const types = Object.fromEntries(out.edges.map((e: any) => [e.id, e.type]));
		expect(types).toEqual({
			[`linked_page:page:${p1}:task:${t}`]: 'linked_page',
			[`links_to:page:${p1}:page:${p2}`]: 'links_to',
			[`links_to:page:${p3}:page:${p1}`]: 'links_to',
		});
		const edge = out.edges.find((e: any) => e.type === 'linked_page');
		expect(edge.why).toBe('Page One links Task');
		const task1 = await ok(`graph/related?entity=task:${t}&depth=1`);
		expect(ids(task1.nodes).sort()).toEqual(
			[`page:${p1}`, `page:${p2}`, `task:${t}`].sort(),
		);
		expect(task1.edges.map((e: any) => e.type).sort()).toEqual([
			'linked_page',
			'mentioned_in',
		]);
	});

	it('shows a persona assignee as a leaf that is not expanded', async () => {
		await persona('uuid-1', 'Analyst');
		const a = await task('A');
		const b = await task('B');
		await assign(a, 'uuid-1');
		await assign(b, 'uuid-1');
		await relate(a, b, 'relates to');
		const hop1 = await ok(`graph/related?entity=task:${a}&depth=1`);
		const leaf = hop1.nodes.find((n: any) => n.type === 'persona');
		expect(leaf).toMatchObject({
			id: 'persona:uuid-1',
			title: 'Analyst',
			rel: 'assignee',
			hop: 1,
		});
		const c = await task('C');
		await assign(c, 'uuid-1');
		await relate(b, c, 'relates to');
		const hop2 = await ok(`graph/related?entity=task:${a}&depth=2`);
		expect(ids(hop2.nodes)).toContain(`task:${c}`);
		expect(hop2.nodes.filter((n: any) => n.type === 'persona')).toHaveLength(1);
		const other = await task('Other');
		await assign(other, 'uuid-1');
		expect(ids(hop2.nodes)).not.toContain(`task:${other}`);
	});

	it('hides disabled and archived personas', async () => {
		await persona('uuid-1', 'Gone');
		await run(`UPDATE workspace_personas SET disabled_at = ?`, [stamp()]);
		const a = await task('A');
		await assign(a, 'uuid-1');
		const out = await ok(`graph/related?entity=task:${a}`);
		expect(out.nodes).toHaveLength(1);
	});

	it('limits depth to 1 or 2 hops', async () => {
		const a = await task('A');
		const b = await task('B');
		const c = await task('C');
		const d = await task('D');
		await relate(a, b, 'relates to');
		await relate(b, c, 'relates to');
		await relate(c, d, 'relates to');
		const one = await ok(`graph/related?entity=task:${a}&depth=1`);
		const two = await ok(`graph/related?entity=task:${a}&depth=2`);
		const big = await ok(`graph/related?entity=task:${a}&depth=9`);
		expect(ids(one.nodes)).toEqual([`task:${a}`, `task:${b}`]);
		expect(ids(two.nodes)).toEqual([`task:${a}`, `task:${b}`, `task:${c}`]);
		expect(ids(big.nodes)).toEqual(ids(two.nodes));
		expect(two.nodes[2].hop).toBe(2);
	});

	it('caps neighbours per edge type and reports caps_hit', async () => {
		const a = await task('Hub');
		for (let i = 0; i < 30; i++) {
			const t = await task(`T${i}`);
			await relate(a, t, 'relates to');
		}
		const out = await ok(`graph/related?entity=task:${a}&depth=1&limit=500`);
		expect(out.nodes).toHaveLength(25);
		expect(out.truncated).toBe(true);
		expect(out.caps_hit).toEqual({ task: 6 });
	});

	it('caps hop-2 nodes per parent at 12', async () => {
		const a = await task('A');
		const b = await task('B');
		await relate(a, b, 'relates to');
		for (let i = 0; i < 10; i++) {
			const t = await task(`R${i}`);
			await relate(b, t, 'relates to');
		}
		for (let i = 0; i < 10; i++) {
			const p = await page(`P${i}`);
			await link(p, 'task', b);
		}
		const out = await ok(`graph/related?entity=task:${a}`);
		expect(out.nodes.filter((n: any) => n.hop === 2)).toHaveLength(12);
		expect(out.truncated).toBe(true);
		const dropped = Object.values(
			out.caps_hit as Record<string, number>,
		).reduce((x, y) => x + y, 0);
		expect(dropped).toBe(8);
	});

	it('honors the total limit and keeps the highest-ranked nodes', async () => {
		const a = await task('A');
		const blockers = [];
		for (let i = 0; i < 3; i++) {
			const t = await task(`Blocked${i}`);
			await relate(a, t, 'blocks');
			blockers.push(t);
		}
		for (let i = 0; i < 5; i++) {
			const t = await task(`Rel${i}`);
			await relate(a, t, 'relates to');
		}
		const out = await ok(`graph/related?entity=task:${a}&depth=1&limit=4`);
		expect(out.nodes).toHaveLength(4);
		expect(out.truncated).toBe(true);
		expect(out.caps_hit).toEqual({ task: 5 });
		expect(out.nodes.slice(1).every((n: any) => n.rel === 'blocks')).toBe(true);
		expect(out.edges.every((e: any) => ids(out.nodes).includes(e.to))).toBe(
			true,
		);
		const again = await ok(`graph/related?entity=task:${a}&depth=1&limit=4`);
		expect(again).toEqual(out);
	});

	it('orders equal-weight neighbours by updated_at desc then id', async () => {
		const a = await task('A');
		const old = await task('Old');
		const fresh = await task('Fresh');
		await relate(a, old, 'relates to');
		await relate(a, fresh, 'relates to');
		await run(
			`UPDATE tasks SET updated_at = '2030-01-01T00:00:00.000Z' WHERE id = ?`,
			[fresh],
		);
		const out = await ok(`graph/related?entity=task:${a}&depth=1`);
		expect(ids(out.nodes)).toEqual([
			`task:${a}`,
			`task:${fresh}`,
			`task:${old}`,
		]);
	});

	it('filters deleted tasks, pages and comments', async () => {
		const a = await task('A');
		const b = await task('B');
		const p = await page('P');
		await relate(a, b, 'relates to');
		await mention(a, p);
		await run(
			`INSERT INTO comments (task_id, message, created_at, updated_at, deleted_at) VALUES (?, 'x', ?, ?, ?)`,
			[a, stamp(), stamp(), stamp()],
		);
		await run(`UPDATE tasks SET deleted_at = ? WHERE id = ?`, [stamp(), b]);
		await run(`UPDATE pages SET deleted_at = ? WHERE id = ?`, [stamp(), p]);
		const out = await ok(
			`graph/related?entity=task:${a}&include=tasks,pages,comments`,
		);
		expect(ids(out.nodes)).toEqual([`task:${a}`]);
		expect(out.edges).toEqual([]);
	});

	it('applies the include filter and opt-in comments', async () => {
		await persona('uuid-1', 'Analyst');
		const a = await task('A');
		const b = await task('B');
		const p = await page('P');
		await relate(a, b, 'relates to');
		await mention(a, p);
		await assign(a, 'uuid-1');
		await run(
			`INSERT INTO comments (task_id, message, created_at, updated_at) VALUES (?, 'Hello comment', ?, ?)`,
			[a, stamp(), stamp()],
		);
		const dflt = await ok(`graph/related?entity=task:${a}`);
		expect(dflt.nodes.map((n: any) => n.type).sort()).toEqual([
			'page',
			'persona',
			'task',
			'task',
		]);
		const tasksOnly = await ok(`graph/related?entity=task:${a}&include=tasks`);
		expect(ids(tasksOnly.nodes)).toEqual([`task:${a}`, `task:${b}`]);
		const withComments = await ok(
			`graph/related?entity=task:${a}&include=comments,people`,
		);
		expect(withComments.nodes.map((n: any) => n.type).sort()).toEqual([
			'comment',
			'task',
			'user',
		]);
		expect(withComments.edges.map((e: any) => e.type).sort()).toEqual([
			'commented',
			'on_task',
		]);
		expect(
			withComments.nodes.find((n: any) => n.type === 'comment').title,
		).toBe('Hello comment');
	});

	it('links a user mention on a page to the local user', async () => {
		const p = await page('P');
		await link(p, 'user', 7);
		await link(p, 'user', 99);
		const out = await ok(`graph/related?entity=page:${p}&depth=1`);
		expect(ids(out.nodes)).toEqual([`page:${p}`, 'user:7']);
		expect(out.nodes[1]).toMatchObject({
			type: 'user',
			title: 'Me',
			rel: 'mentions',
		});
		expect(JSON.stringify(out)).not.toContain('example.com');
	});

	it('finds a shortest path across tasks and pages and reports no path', async () => {
		const a = await task('A');
		const b = await task('B');
		const c = await task('C');
		const lone = await task('Lone');
		const p = await page('P');
		await relate(a, b, 'blocks');
		await mention(b, p);
		await link(p, 'task', c);
		const found = await ok(`graph/path?from=task:${a}&to=task:${c}`);
		expect(ids(found.nodes)).toEqual([
			`task:${a}`,
			`task:${b}`,
			`page:${p}`,
			`task:${c}`,
		]);
		expect(found.edges.map((e: any) => e.type)).toEqual([
			'blocks',
			'mentioned_in',
			'linked_page',
		]);
		expect(found.nodes.map((n: any) => n.hop)).toEqual([0, 1, 2, 3]);
		const tooShort = await ok(
			`graph/path?from=task:${a}&to=task:${c}&max_depth=2`,
		);
		expect(tooShort.nodes).toEqual([]);
		const none = await ok(`graph/path?from=task:${a}&to=task:${lone}`);
		expect(none).toEqual({
			from: `task:${a}`,
			to: `task:${lone}`,
			nodes: [],
			edges: [],
		});
		const self = await ok(`graph/path?from=task:${a}&to=task:${a}`);
		expect(ids(self.nodes)).toEqual([`task:${a}`]);
		expect((await get(`graph/path?from=task:${a}&to=task:999`)).status).toBe(
			404,
		);
	});

	it('does not route paths through personas', async () => {
		await persona('uuid-1', 'Analyst');
		const a = await task('A');
		const b = await task('B');
		await assign(a, 'uuid-1');
		await assign(b, 'uuid-1');
		const out = await ok(`graph/path?from=task:${a}&to=task:${b}`);
		expect(out.nodes).toEqual([]);
	});

	it('ranks hubs by degree and bottlenecks by dependants', async () => {
		const hub = await task('Hub');
		const blocker = await task('Blocker');
		const x = await task('X');
		const y = await task('Y');
		const z = await task('Z');
		const p = await page('P');
		await relate(hub, x, 'relates to');
		await relate(hub, y, 'relates to');
		await relate(hub, blocker, 'relates to');
		await mention(hub, p);
		await relate(blocker, x, 'blocks');
		await relate(z, blocker, 'depends on');
		await relate(blocker, y, 'is blocked by');
		const out = await ok('graph/hubs?limit=3');
		expect(out.hubs.map((h: any) => h.node.id)).toEqual([
			`task:${blocker}`,
			`task:${hub}`,
			`task:${y}`,
		]);
		expect(out.hubs[1]).toMatchObject({ degree: 4, blocks: 0 });
		expect(out.bottlenecks[0]).toMatchObject({ blocks: 2 });
		expect(out.bottlenecks[0].node.id).toBe(`task:${blocker}`);
		expect(out.bottlenecks.map((b: any) => b.node.id)).toEqual([
			`task:${blocker}`,
			`task:${y}`,
		]);
	});

	it('lists orphans without completed or archived tasks and counts the total', async () => {
		const active = await status('Doing', 'active');
		const completed = await status('Done', 'completed');
		const archived = await status('Old', 'archived');
		const a = await task('Linked A', { status: active });
		const b = await task('Linked B');
		await relate(a, b, 'relates to');
		const lonely = await task('Lonely', { status: active });
		await task('Finished', { status: completed });
		await task('Shelved', { status: archived });
		const noStatus = await task('No status');
		const lonePage = await page('Lone page');
		const linked = await page('Linked page');
		await mention(a, linked);
		const deleted = await task('Deleted');
		await run(`UPDATE tasks SET deleted_at = ? WHERE id = ?`, [
			stamp(),
			deleted,
		]);
		const out = await ok('graph/orphans');
		expect(out.total).toBe(3);
		expect(ids(out.nodes).sort()).toEqual(
			[`page:${lonePage}`, `task:${lonely}`, `task:${noStatus}`].sort(),
		);
		const limited = await ok('graph/orphans?limit=1');
		expect(limited.nodes).toHaveLength(1);
		expect(limited.total).toBe(3);
	});

	it('does not count an edge to a deleted task as a connection', async () => {
		const a = await task('A');
		const b = await task('B');
		await relate(a, b, 'relates to');
		await run(`UPDATE tasks SET deleted_at = ? WHERE id = ?`, [stamp(), b]);
		const out = await ok('graph/orphans');
		expect(ids(out.nodes)).toEqual([`task:${a}`]);
		const hubs = await ok('graph/hubs');
		expect(hubs.hubs).toEqual([]);
	});

	it('closes the graph to personas by default', async () => {
		const a = await task('A');
		const res = await get(`graph/related?entity=task:${a}`, {
			kind: 'persona',
			id: 'p',
			name: 'P',
		});
		expect([401, 403]).toContain(res.status);
	});
});

describeSqlite('local graph with pages off', () => {
	it('drops page nodes and 404s on page entities', async () => {
		const { related } = await import('../service');
		const ctx = {
			db: memoryDb(),
			workspace: {
				id: -1,
				name: 'x',
				code: 'local-x',
				schema_version: 0,
				created_at: '',
				path: '',
				database: '',
			},
			user: { id: 7, name: 'Me', email: 'me@example.com' },
			now: () => new Date('2026-10-01T10:00:00.000Z'),
			files: {
				url: () => '',
				read: async () => new Blob(),
				remove: async () => {},
			},
		} as LocalContext;
		await migrate(ctx.db, '2026-10-01T10:00:00.000Z');
		await ctx.db.execute(
			`INSERT INTO tasks (title, created_at, updated_at) VALUES ('T', 'a', 'a')`,
		);
		await ctx.db.execute(
			`INSERT INTO pages (title, slug, type, author_id, author_kind, author_ref, updated_by_id, updated_by_kind, updated_by_ref, created_at, updated_at)
			 VALUES ('P', 'p', 'plain', 1, 'user', '1', 1, 'user', '1', 'a', 'a')`,
		);
		await ctx.db.execute(
			`INSERT INTO task_page_mentions (task_id, page_id) VALUES (1, 1)`,
		);
		const off = { pagesEnabled: false };
		const out = await related(ctx, { entity: 'task:1' }, off);
		expect(out.nodes.map((n) => n.id)).toEqual(['task:1']);
		await expect(related(ctx, { entity: 'page:1' }, off)).rejects.toMatchObject(
			{ status: 404 },
		);
		await expect(related(ctx, { entity: 'page:p' }, off)).rejects.toMatchObject(
			{ status: 404 },
		);
		const on = await related(ctx, { entity: 'task:1' }, { pagesEnabled: true });
		expect(on.nodes.map((n) => n.id)).toEqual(['task:1', 'page:1']);
	});
});
