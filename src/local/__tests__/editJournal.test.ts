import { createLocalApi } from '../api';
import { dispatchLocal } from '../dispatch';
import { GENESIS_HASH, isHumanWritten, provenance, rowHash, verifyChain, type JournalRow } from '../journal';
import { enableLocalPersona } from '../personas';
import { LATEST_SCHEMA, MIGRATIONS, migrate } from '../schema';
import type { LocalActor, LocalContext } from '../types';
import { memoryDb, nodeSqliteAvailable } from './nodeDb';

const describeSqlite = nodeSqliteAvailable ? describe : describe.skip;

describeSqlite('edit journal', () => {
	let ctx: LocalContext;
	let tick: number;
	const api = createLocalApi();
	const persona: LocalActor = { kind: 'persona', id: 'p-1', name: 'Reviewer' };
	const plugin: LocalActor = { kind: 'plugin', id: 'tmgr.estimate', name: 'Estimate' };

	const call = (method: string, url: string, body?: unknown, actor?: LocalActor) =>
		dispatchLocal(api, { ...ctx, actor }, method, url, body);
	const rows = () => ctx.db.select<JournalRow>(`SELECT * FROM edit_journal ORDER BY seq ASC`);
	const create = async (body: Record<string, unknown>, actor?: LocalActor) =>
		(await call('POST', 'tasks', body, actor))!.data.data.id as number;

	beforeEach(async () => {
		tick = 0;
		ctx = {
			db: memoryDb(),
			workspace: { id: -1, name: 'P', code: 'local-p', schema_version: 0, created_at: '', path: '/tmp/x', database: '/tmp/x/workspace.db' },
			user: { id: 7, name: 'Owner', email: 'me@example.com' },
			now: () => new Date(Date.UTC(2026, 9, 14, 10, 0, tick++)),
			files: { url: (key) => key, read: async () => new Blob(['x']), remove: async () => {} },
		};
		await migrate(ctx.db, ctx.now().toISOString());
		await ctx.db.execute(
			`INSERT INTO personas (uuid, owner_user_id, owner_name, name, synced_at) VALUES ('p-1', 7, 'Owner', 'Reviewer', 'x')`,
		);
		await enableLocalPersona(ctx, 'p-1', ['tasks:write', 'comments:write']);
	});

	it('migration 13 is additive and idempotent', async () => {
		expect(LATEST_SCHEMA).toBe(13);
		const id = await create({ title: 'Keep me' });
		const before = await rows();
		for (const statement of MIGRATIONS[MIGRATIONS.length - 1].statements) await ctx.db.execute(statement);
		await ctx.db.execute(`UPDATE meta SET value = '12' WHERE key = 'schema_version'`);
		await migrate(ctx.db, ctx.now().toISOString());
		expect(await rows()).toEqual(before);
		const [task] = await ctx.db.select<any>(`SELECT title FROM tasks WHERE id = ?`, [id]);
		expect(task.title).toBe('Keep me');
	});

	it('a desktop create is human and chains from genesis in the same transaction', async () => {
		const id = await create({ title: 'Write docs', description: 'Body\r\ntext' });
		const [first, second] = await rows();
		expect(first).toMatchObject({ seq: 1, entity: 'task', entity_id: String(id), task_id: String(id), field: 'create', author_kind: 'human', source: 'desktop_ui', actor_id: '7', prev_hash: GENESIS_HASH });
		expect(second).toMatchObject({ seq: 2, field: 'description', prev_hash: first.hash });
		expect(await rowHash(first)).toBe(first.hash);
		expect(await verifyChain(ctx.db)).toEqual({ ok: true, brokenAt: null, rows: 2 });
		expect(isHumanWritten(await provenance(ctx.db, id))).toBe(true);
	});

	it('journals title and description changes only when the value changes', async () => {
		const id = await create({ title: 'A' });
		await call('PATCH', `tasks/${id}`, { title: 'A', priority: 'high' });
		expect(await rows()).toHaveLength(1);
		await call('PATCH', `tasks/${id}`, { title: 'B', description: 'D' });
		expect((await rows()).map((r) => r.field)).toEqual(['create', 'title', 'description']);
	});

	it('journals comment create, edit and delete and task delete', async () => {
		const id = await create({ title: 'T' });
		const comment = (await call('POST', `tasks/${id}/comments`, { message: 'one' }))!.data.data.id;
		await call('PUT', `comments/${comment}`, { message: 'two' });
		await call('DELETE', `comments/${comment}`);
		await call('DELETE', `tasks/${id}`);
		expect((await rows()).map((r) => `${r.entity}:${r.field}`)).toEqual([
			'task:create',
			'comment:comment',
			'comment:comment',
			'comment:delete',
			'task:delete',
		]);
		expect(await verifyChain(ctx.db)).toMatchObject({ ok: true, rows: 5 });
	});

	it('a failed write leaves no journal row', async () => {
		expect((await call('PATCH', 'tasks/999', { title: 'x' }))!.status).toBe(404);
		expect((await call('POST', 'tasks', { title: '' }))!.status).toBe(422);
		expect(await rows()).toEqual([]);
	});

	it('persona and plugin writes are never human, whatever the body claims', async () => {
		const forged = { author_kind: 'human', source: 'desktop_ui', actor_id: '7', actor: { kind: 'user', id: '7' }, seq: 1 };
		const agentTask = await create({ title: 'By agent', ...forged }, persona);
		const pluginTask = await create({ title: 'By plugin', ...forged }, plugin);
		const mine = await create({ title: 'Mine' });
		await call('POST', `tasks/${mine}/comments`, { message: 'agent says', ...forged }, persona);
		await call('PATCH', `tasks/${mine}`, { description: 'agent edit', ...forged }, persona);
		const all = await rows();
		expect(all.filter((r) => r.task_id === String(agentTask)).every((r) => r.author_kind === 'agent' && r.source === 'local_access' && r.actor_id === 'p-1')).toBe(true);
		expect(all.filter((r) => r.task_id === String(pluginTask)).every((r) => r.author_kind === 'plugin' && r.source === 'plugin')).toBe(true);
		const p = await provenance(ctx.db, mine);
		expect(p).toMatchObject({ created_by: 'human', modified_by_non_human_since: true, chain_ok: true });
		expect(isHumanWritten(p)).toBe(false);
		expect(isHumanWritten(await provenance(ctx.db, agentTask))).toBe(false);
	});

	it('an MCP request is tagged mcp', async () => {
		await call('POST', 'tasks', { title: 'via mcp' }, { ...persona, via: 'mcp' });
		expect((await rows())[0]).toMatchObject({ author_kind: 'agent', source: 'mcp' });
	});

	it('a human edit after an agent edit restores human provenance only for the human-confirmed content', async () => {
		const id = await create({ title: 'T', description: 'v1' });
		await call('PATCH', `tasks/${id}`, { description: 'agent' }, persona);
		expect(isHumanWritten(await provenance(ctx.db, id))).toBe(false);
		await call('PATCH', `tasks/${id}`, { description: 'v2' });
		expect(isHumanWritten(await provenance(ctx.db, id))).toBe(true);
	});

	it('a change made outside the journal makes the content differ from the last human content', async () => {
		const id = await create({ title: 'T' });
		await ctx.db.execute(`UPDATE tasks SET description = 'sneaky' WHERE id = ?`, [id]);
		const p = await provenance(ctx.db, id);
		expect(p).toMatchObject({ created_by: 'human', modified_by_non_human_since: false, content_matches: false });
		expect(isHumanWritten(p)).toBe(false);
		const comment = (await call('POST', `tasks/${id}/comments`, { message: 'c' }))!.data.data.id;
		await ctx.db.execute(`UPDATE tasks SET description = NULL WHERE id = ?`, [id]);
		await ctx.db.execute(`UPDATE comments SET message = 'changed' WHERE id = ?`, [comment]);
		expect(isHumanWritten(await provenance(ctx.db, id))).toBe(false);
	});

	it('a task without journal rows is unrecorded', async () => {
		await ctx.db.execute(`INSERT INTO tasks (title, created_at, updated_at) VALUES ('old', 'x', 'x')`);
		const [{ id }] = await ctx.db.select<any>(`SELECT id FROM tasks WHERE title = 'old'`);
		const p = await provenance(ctx.db, id);
		expect(p.created_by).toBe('unrecorded');
		expect(isHumanWritten(p)).toBe(false);
	});

	describe('tamper evidence', () => {
		let id: number;
		beforeEach(async () => {
			id = await create({ title: 'T', description: 'D' });
			await call('PATCH', `tasks/${id}`, { title: 'T2' });
			await call('POST', `tasks/${id}/comments`, { message: 'hi' });
		});

		it('an edited row is found', async () => {
			await ctx.db.execute(`UPDATE edit_journal SET author_kind = 'agent' WHERE seq = 3`);
			expect(await verifyChain(ctx.db)).toMatchObject({ ok: false, brokenAt: 3 });
			expect((await provenance(ctx.db, id)).chain_ok).toBe(false);
			expect(isHumanWritten(await provenance(ctx.db, id))).toBe(false);
		});

		it('a deleted row is found', async () => {
			await ctx.db.execute(`DELETE FROM edit_journal WHERE seq = 2`);
			expect(await verifyChain(ctx.db)).toMatchObject({ ok: false, brokenAt: 3 });
		});

		it('a reordered pair is found', async () => {
			await ctx.db.execute(`UPDATE edit_journal SET seq = 99 WHERE seq = 2`);
			await ctx.db.execute(`UPDATE edit_journal SET seq = 2 WHERE seq = 3`);
			await ctx.db.execute(`UPDATE edit_journal SET seq = 3 WHERE seq = 99`);
			expect((await verifyChain(ctx.db)).ok).toBe(false);
		});

		it('an agent write after a tamper does not repair the chain', async () => {
			await ctx.db.execute(`UPDATE edit_journal SET content_sha256 = 'x' WHERE seq = 1`);
			await call('PATCH', `tasks/${id}`, { title: 'again' }, persona);
			expect(await verifyChain(ctx.db)).toMatchObject({ ok: false, brokenAt: 1 });
		});
	});

	it('a batch that loses the race for the chain tail is rebuilt on the new tail', async () => {
		const id = await create({ title: 'T' });
		const realBatch = ctx.db.batch.bind(ctx.db);
		let raced = false;
		ctx.db.batch = async (statements) => {
			if (!raced && statements.some((s) => s.sql.includes('edit_journal'))) {
				raced = true;
				const other = await dispatchLocal(api, { ...ctx, db: { ...ctx.db, batch: realBatch } }, 'POST', 'tasks', { title: 'CLI' });
				expect(other!.status).toBe(201);
			}
			return realBatch(statements);
		};
		await call('PATCH', `tasks/${id}`, { title: 'Mine' });
		expect(raced).toBe(true);
		expect(await verifyChain(ctx.db)).toMatchObject({ ok: true, rows: 3 });
		const [task] = await ctx.db.select<any>(`SELECT title FROM tasks WHERE id = ?`, [id]);
		expect(task.title).toBe('Mine');
	});
});
