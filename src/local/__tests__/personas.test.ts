import {
	disableLocalPersona,
	enableLocalPersona,
	listLocalPersonas,
	syncPersonasSnapshot,
	type CloudPersona,
} from '../personas';
import { LATEST_SCHEMA, MIGRATIONS, migrate, readSchemaVersion } from '../schema';
import type { LocalContext } from '../types';
import { memoryDb, nodeSqliteAvailable } from './nodeDb';

const describeSqlite = nodeSqliteAvailable ? describe : describe.skip;

describeSqlite('local personas snapshot and gate state', () => {
	let ctx: LocalContext;
	const clock = new Date('2026-09-27T10:00:00Z');

	beforeEach(async () => {
		ctx = {
			db: memoryDb(),
			workspace: { id: -1, name: 'Personal', code: 'local-personal', schema_version: 0, created_at: '', path: '/tmp/x', database: '/tmp/x/workspace.db' },
			user: { id: 7, name: 'Yurij', email: 'me@example.com' },
			now: () => clock,
			files: { url: (key) => `tmgrfile://localhost/${key}`, read: async () => new Blob(['x']), remove: async () => {} },
		};
		await migrate(ctx.db, clock.toISOString());
	});

	const persona = (overrides: Partial<CloudPersona> = {}): CloudPersona => ({
		id: 'p-1',
		name: 'Reviewer',
		description: 'Reviews things',
		avatar_url: null,
		archived_at: null,
		owner: { id: 7, name: 'Yurij' },
		system_prompt: 'You are a careful reviewer.',
		prompt_version: 3,
		...overrides,
	});

	it('upserts identity rows and never writes the prompt to workspace.db', async () => {
		const cached: Record<string, unknown> = {};
		const cache = { put: async (uuid: string, data: unknown) => { cached[uuid] = data; } };
		const result = await syncPersonasSnapshot(ctx, async () => [persona()], cache);
		expect(result.synced).toBe(1);

		const rows = await ctx.db.select<any>(`SELECT * FROM personas WHERE uuid = 'p-1'`);
		expect(rows).toHaveLength(1);
		expect(rows[0].name).toBe('Reviewer');
		expect(rows[0].archived_at).toBeNull();
		const columns = Object.keys(rows[0]);
		expect(columns).not.toContain('system_prompt');
		expect(columns).not.toContain('prompt_version');
		expect(JSON.stringify(rows[0])).not.toMatch(/careful reviewer/);

		expect(cached['p-1']).toEqual({ system_prompt: 'You are a careful reviewer.', prompt_version: 3 });
	});

	it('archives locally when the cloud persona is archived', async () => {
		const cache = { put: async () => {} };
		await syncPersonasSnapshot(ctx, async () => [persona()], cache);
		await syncPersonasSnapshot(ctx, async () => [persona({ archived_at: '2026-09-27T11:00:00Z' })], cache);
		const [row] = await ctx.db.select<any>(`SELECT archived_at FROM personas WHERE uuid = 'p-1'`);
		expect(row.archived_at).toBe('2026-09-27T11:00:00Z');
	});

	it('enables with permissions, lists the grant, then disables offline without touching the row', async () => {
		const cache = { put: async () => {} };
		await syncPersonasSnapshot(ctx, async () => [persona()], cache);
		await enableLocalPersona(ctx, 'p-1', ['tasks:read', 'comments:write']);

		let [row] = await listLocalPersonas(ctx);
		expect(row.permissions).toEqual(['tasks:read', 'comments:write']);
		expect(row.disabled_at).toBeNull();

		await disableLocalPersona(ctx, 'p-1');
		[row] = await listLocalPersonas(ctx);
		expect(row.disabled_at).toBe(clock.toISOString());
		expect(row.permissions).toEqual(['tasks:read', 'comments:write']);
	});

	it('rejects an unknown permission and enabling a persona missing from the snapshot', async () => {
		const cache = { put: async () => {} };
		await syncPersonasSnapshot(ctx, async () => [persona()], cache);
		await expect(enableLocalPersona(ctx, 'p-1', ['tasks:delete' as any])).rejects.toThrow('Unknown permission');
		await expect(enableLocalPersona(ctx, 'missing', ['tasks:read'])).rejects.toThrow('not found');
	});

	it('migration 7 adds the identity-only personas tables on top of a v6 db, keeps old data, and re-runs safely', async () => {
		const fresh = memoryDb();
		const now = clock.toISOString();
		for (const migration of MIGRATIONS.filter((m) => m.version <= 6)) {
			for (const statement of migration.statements) await fresh.execute(statement);
		}
		await fresh.execute(
			`INSERT INTO meta (key, value) VALUES ('schema_version', ?) ON CONFLICT(key) DO UPDATE SET value = excluded.value`,
			['6'],
		);
		expect(await readSchemaVersion(fresh)).toBe(6);

		await fresh.execute(
			`INSERT INTO tasks (id, title, priority, created_at, updated_at) VALUES (1, 'Pre-existing task', 'medium', ?, ?)`,
			[now, now],
		);
		await fresh.execute(
			`INSERT INTO comments (task_id, message, author_kind, created_at, updated_at) VALUES (1, 'Pre-existing comment', 'user', ?, ?)`,
			[now, now],
		);

		expect(await migrate(fresh, now)).toBe(LATEST_SCHEMA);

		const personaColumns = (await fresh.select<any>(`PRAGMA table_info(personas)`)).map((c: any) => c.name);
		expect(personaColumns).toEqual([
			'uuid',
			'owner_user_id',
			'owner_name',
			'name',
			'description',
			'avatar_file',
			'synced_at',
			'archived_at',
		]);
		const grantColumns = (await fresh.select<any>(`PRAGMA table_info(workspace_personas)`)).map((c: any) => c.name);
		expect(grantColumns).toEqual(['persona_uuid', 'permissions', 'enabled_at', 'disabled_at']);

		const [task] = await fresh.select<any>(`SELECT title FROM tasks WHERE id = 1`);
		expect(task.title).toBe('Pre-existing task');
		const [comment] = await fresh.select<any>(`SELECT message FROM comments WHERE task_id = 1`);
		expect(comment.message).toBe('Pre-existing comment');

		await fresh.execute(`UPDATE meta SET value = '6' WHERE key = 'schema_version'`);
		await expect(migrate(fresh, now)).resolves.toBe(LATEST_SCHEMA);
		expect(await readSchemaVersion(fresh)).toBe(LATEST_SCHEMA);
	});
});
