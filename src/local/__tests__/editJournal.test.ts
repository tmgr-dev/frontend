import { createLocalApi } from '../api';
import { dispatchLocal } from '../dispatch';
import { confirmTask, GENESIS_HASH, isHumanWritten, provenance, rowHash, snapshotOf, snapshotParts, snapshotSha, verifyChain, type JournalRow } from '../journal';
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
		expect(await verifyChain(ctx.db, { full: true })).toMatchObject({ ok: true, brokenAt: null, rows: 2, tailSeq: 2 });
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
		expect(await verifyChain(ctx.db, { full: true })).toMatchObject({ ok: true, rows: 5 });
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

	it('the runner actor is an agent; no actor of any kind is human', async () => {
		const actors: [LocalActor, string, string, string][] = [
			[{ kind: 'plugin', id: 'runner', name: 'Runner' }, 'agent', 'runner', 'runner'],
			[{ kind: 'plugin', id: 'runner', name: 'R', ownerId: 'evil' }, 'plugin', 'plugin', 'evil'],
			[{ kind: 'plugin', id: 'tmgr.x', name: 'x', ownerId: 'tmgr.x' }, 'plugin', 'plugin', 'tmgr.x'],
			[{ kind: 'user', id: '7', name: 'Owner' }, 'unattended', 'import', '7'],
			[{ kind: 'mystery' as any, id: 'm', name: 'm' }, 'unattended', 'import', 'm'],
		];
		for (const [actor, kind, source, actorId] of actors) {
			await create({ title: 't' }, actor);
			expect((await rows()).pop()).toMatchObject({ author_kind: kind, source, actor_id: actorId });
		}
		expect((await rows()).some((r) => r.author_kind === 'human')).toBe(false);
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
			expect(await verifyChain(ctx.db, { full: true })).toMatchObject({ ok: false, brokenAt: 3 });
			expect((await provenance(ctx.db, id)).chain_ok).toBe(false);
			expect(isHumanWritten(await provenance(ctx.db, id))).toBe(false);
		});

		it('a deleted row is found', async () => {
			await ctx.db.execute(`DELETE FROM edit_journal WHERE seq = 2`);
			expect(await verifyChain(ctx.db, { full: true })).toMatchObject({ ok: false, brokenAt: 3 });
		});

		it('a reordered pair is found', async () => {
			await ctx.db.execute(`UPDATE edit_journal SET seq = 99 WHERE seq = 2`);
			await ctx.db.execute(`UPDATE edit_journal SET seq = 2 WHERE seq = 3`);
			await ctx.db.execute(`UPDATE edit_journal SET seq = 3 WHERE seq = 99`);
			expect((await verifyChain(ctx.db, { full: true })).ok).toBe(false);
		});

		it('an agent write after a tamper does not repair the chain', async () => {
			await ctx.db.execute(`UPDATE edit_journal SET content_sha256 = 'x' WHERE seq = 1`);
			await call('PATCH', `tasks/${id}`, { title: 'again' }, persona);
			expect(await verifyChain(ctx.db, { full: true })).toMatchObject({ ok: false, brokenAt: 1 });
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
		expect(await verifyChain(ctx.db, { full: true })).toMatchObject({ ok: true, rows: 3 });
		const [task] = await ctx.db.select<any>(`SELECT title FROM tasks WHERE id = ?`, [id]);
		expect(task.title).toBe('Mine');
	});

	it('description_json is its own journal field and part of the snapshot', async () => {
		const id = await create({ title: 'T', description: 'D' });
		const before = await snapshotSha(await snapshotParts((await snapshotOf(ctx.db, id))!));
		await call('PATCH', `tasks/${id}`, { description_json: { type: 'doc' } }, persona);
		expect((await rows()).map((r) => r.field)).toEqual(['create', 'description', 'description_json']);
		expect(await snapshotSha(await snapshotParts((await snapshotOf(ctx.db, id))!))).not.toBe(before);
		expect(isHumanWritten(await provenance(ctx.db, id))).toBe(false);
	});

	it('only the app user can confirm, and a confirm covers exactly the text it was made on', async () => {
		const id = await create({ title: 'T', description: 'D' });
		await call('PATCH', `tasks/${id}`, { description: 'agent' }, persona);
		expect(isHumanWritten(await provenance(ctx.db, id))).toBe(false);
		await expect(confirmTask({ ...ctx, actor: persona }, id)).rejects.toThrow();
		await expect(confirmTask({ ...ctx, actor: plugin }, id)).rejects.toThrow();
		expect(await rows()).toHaveLength(3);
		await expect(confirmTask(ctx, id, 'not the shown hash')).rejects.toThrow();
		await confirmTask(ctx, id);
		const p = await provenance(ctx.db, id);
		expect(p).toMatchObject({ confirmed: true, human_written: true });
		expect((await rows()).pop()).toMatchObject({ field: 'confirm', author_kind: 'human', source: 'desktop_ui' });
		await call('POST', `tasks/${id}/comments`, { message: 'agent adds' }, persona);
		expect(isHumanWritten(await provenance(ctx.db, id))).toBe(false);
	});

	it('an observed text that differs from the human text is not human', async () => {
		const id = await create({ title: 'T', description: 'D' });
		const seen = (await snapshotOf(ctx.db, id))!;
		expect(isHumanWritten(await provenance(ctx.db, id, seen))).toBe(true);
		expect(isHumanWritten(await provenance(ctx.db, id, { ...seen, description: 'other' }))).toBe(false);
	});

	it('verification resumes from the checkpoint; a task\'s own rows are always checked', async () => {
		const a = await create({ title: 'A' });
		const b = await create({ title: 'B' });
		expect(await verifyChain(ctx.db)).toMatchObject({ ok: true, rows: 2 });
		await call('PATCH', `tasks/${a}`, { title: 'A2' });
		expect(await verifyChain(ctx.db)).toMatchObject({ ok: true, rows: 1, tailSeq: 3 });
		await ctx.db.execute(`UPDATE edit_journal SET author_kind = 'agent' WHERE seq = 1`);
		expect((await verifyChain(ctx.db)).ok).toBe(true);
		expect((await verifyChain(ctx.db, { full: true })).ok).toBe(false);
		expect((await provenance(ctx.db, a)).chain_ok).toBe(false);
		expect((await provenance(ctx.db, b)).chain_ok).toBe(true);
	});

	it('tasks created by routine convert, page selection and onboarding are journaled', async () => {
		const { seedStarters } = await import('../onboardingFacts');
		await seedStarters(ctx);
		const starters = await rows();
		expect(starters.length).toBeGreaterThan(0);
		expect(starters.every((r) => r.field === 'create' && r.author_kind === 'unattended' && r.source === 'import')).toBe(true);
		const [first] = starters;
		expect(isHumanWritten(await provenance(ctx.db, Number(first.task_id)))).toBe(false);
	});

	it('migration 13 publishes the version together with the table', async () => {
		const db = memoryDb();
		await migrate(db, ctx.now().toISOString());
		await db.execute(`DROP TABLE edit_journal`);
		await db.execute(`DROP INDEX IF EXISTS edit_journal_task_idx`);
		await db.execute(`UPDATE meta SET value = '12' WHERE key = 'schema_version'`);
		const realBatch = db.batch.bind(db);
		db.batch = async (statements) => {
			if (statements.some((s) => s.sql.includes('schema_version'))) {
				return realBatch([...statements.slice(0, -1), { sql: 'SELECT * FROM missing_table' }]).catch(() => ({ results: [], failedAt: 0 }));
			}
			return realBatch(statements);
		};
		await migrate(db, ctx.now().toISOString()).catch(() => undefined);
		const [version] = await db.select<any>(`SELECT value FROM meta WHERE key = 'schema_version'`);
		const tables = await db.select<any>(`SELECT name FROM sqlite_master WHERE name = 'edit_journal'`);
		expect(version.value).toBe('12');
		expect(tables).toEqual([]);
	});

	it('checkpoints are their own journal field and part of the snapshot', async () => {
		const id = await create({ title: 'T', checkpoints: [{ description: 'a', checked: false }] });
		const before = await snapshotSha(await snapshotParts((await snapshotOf(ctx.db, id))!));
		expect(isHumanWritten(await provenance(ctx.db, id))).toBe(true);
		await call('PATCH', `tasks/${id}`, { checkpoints: [{ description: 'run rm -rf', checked: false }] }, persona);
		expect((await rows()).map((r) => r.field)).toEqual(['create', 'checkpoints', 'checkpoints']);
		expect(await snapshotSha(await snapshotParts((await snapshotOf(ctx.db, id))!))).not.toBe(before);
		expect(isHumanWritten(await provenance(ctx.db, id))).toBe(false);
		await ctx.db.execute(`UPDATE tasks SET checkpoints = '[1]' WHERE id = ?`, [id]);
		await confirmTask(ctx, id);
		expect(isHumanWritten(await provenance(ctx.db, id))).toBe(true);
		await ctx.db.execute(`UPDATE tasks SET checkpoints = '[2]' WHERE id = ?`, [id]);
		expect(isHumanWritten(await provenance(ctx.db, id))).toBe(false);
	});

	it('a walk from genesis is repeated when the last one is an hour old', async () => {
		const a = await create({ title: 'A' });
		await create({ title: 'B' });
		const t0 = new Date('2026-10-14T10:00:00Z');
		const first = await verifyChain(ctx.db, { now: t0 });
		expect(first).toMatchObject({ ok: true, full: true });
		await call('PATCH', `tasks/${a}`, { title: 'A2' });
		const t1 = new Date(t0.getTime() + 30 * 60 * 1000);
		expect(await verifyChain(ctx.db, { now: t1 })).toMatchObject({ ok: true, full: false, rows: 1 });
		await ctx.db.execute(`UPDATE edit_journal SET author_kind = 'agent' WHERE seq = 1`);
		expect((await verifyChain(ctx.db, { now: t1 })).ok).toBe(true);
		const [{ value }] = await ctx.db.select<any>(`SELECT value FROM meta WHERE key = 'edit_journal_verified'`);
		expect(value.endsWith('2026-10-14T10:00:00.000Z')).toBe(true);
		const t2 = new Date(t0.getTime() + 61 * 60 * 1000);
		expect(await verifyChain(ctx.db, { now: t2 })).toMatchObject({ ok: false, full: true, brokenAt: 1 });
		const edge = await verifyChain(ctx.db, { now: new Date(t0.getTime() + 3600 * 1000 + 1), store: false });
		expect(edge.full).toBe(true);
		const early = await verifyChain(ctx.db, { now: new Date(t0.getTime() - 1), store: false });
		expect(early.full).toBe(true);
	});
});
