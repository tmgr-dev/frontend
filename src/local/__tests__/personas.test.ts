import {
	disableLocalPersona,
	enableLocalPersona,
	listLocalPersonas,
	syncPersonasSnapshot,
	type CloudPersona,
} from '../personas';
import { migrate } from '../schema';
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
});
