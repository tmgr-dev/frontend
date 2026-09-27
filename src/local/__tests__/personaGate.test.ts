import { createLocalApi } from '../api';
import { dispatchLocal } from '../dispatch';
import { enableLocalPersona } from '../personas';
import { migrate } from '../schema';
import type { LocalContext } from '../types';
import { memoryDb, nodeSqliteAvailable } from './nodeDb';

const describeSqlite = nodeSqliteAvailable ? describe : describe.skip;

describeSqlite('persona whitelist gate in the local router', () => {
	let ctx: LocalContext;
	let taskId: number;
	const clock = new Date('2026-09-27T10:00:00Z');
	const api = createLocalApi();
	const persona = { kind: 'persona' as const, id: 'p-1', name: 'Reviewer' };

	const call = (method: string, url: string, body?: unknown, actor = ctx.actor) =>
		dispatchLocal(api, { ...ctx, actor }, method, url, body);

	beforeEach(async () => {
		ctx = {
			db: memoryDb(),
			workspace: { id: -1, name: 'Personal', code: 'local-personal', schema_version: 0, created_at: '', path: '/tmp/x', database: '/tmp/x/workspace.db' },
			user: { id: 7, name: 'Yurij', email: 'me@example.com' },
			now: () => clock,
			files: { url: (key) => `tmgrfile://localhost/${key}`, read: async () => new Blob(['x']), remove: async () => {} },
		};
		await migrate(ctx.db, clock.toISOString());
		await ctx.db.execute(
			`INSERT INTO personas (uuid, owner_user_id, owner_name, name, description, avatar_file, synced_at, archived_at)
			 VALUES ('p-1', 7, 'Yurij', 'Reviewer', NULL, NULL, ?, NULL)`,
			[clock.toISOString()],
		);
		const created = await dispatchLocal(api, ctx, 'POST', 'tasks', { title: 'Do the thing' });
		taskId = created!.data.data.id;
	});

	it('allows a whitelisted read once the persona has the permission', async () => {
		await enableLocalPersona(ctx, 'p-1', ['tasks:read']);
		const res = await call('GET', `tasks/${taskId}`, undefined, persona);
		expect(res!.status).toBe(200);
	});

	it('allows a whitelisted write and attributes it to the persona', async () => {
		await enableLocalPersona(ctx, 'p-1', ['comments:write']);
		const res = await call('POST', `tasks/${taskId}/comments`, { message: 'Looks fine' }, persona);
		expect(res!.status).toBe(201);
		expect(res!.data.data.author).toEqual({
			kind: 'persona',
			id: 'p-1',
			name: 'Reviewer',
			owner: { id: '7', name: 'Yurij' },
		});
	});

	it('returns 403 when the grant is missing the required permission', async () => {
		await enableLocalPersona(ctx, 'p-1', ['comments:read']);
		const res = await call('POST', `tasks/${taskId}/comments`, { message: 'nope' }, persona);
		expect(res!.status).toBe(403);
	});

	it('returns 401 when the persona was disabled locally, even offline', async () => {
		await enableLocalPersona(ctx, 'p-1', ['tasks:read']);
		const { disableLocalPersona } = await import('../personas');
		await disableLocalPersona(ctx, 'p-1');
		const res = await call('GET', `tasks/${taskId}`, undefined, persona);
		expect(res!.status).toBe(401);
	});

	it('returns 401 when the persona is archived', async () => {
		await enableLocalPersona(ctx, 'p-1', ['tasks:read']);
		await ctx.db.execute(`UPDATE personas SET archived_at = ? WHERE uuid = 'p-1'`, [clock.toISOString()]);
		const res = await call('GET', `tasks/${taskId}`, undefined, persona);
		expect(res!.status).toBe(401);
	});

	it('returns 403 for a route outside the whitelist even with full-looking permissions', async () => {
		await enableLocalPersona(ctx, 'p-1', ['tasks:read', 'tasks:write']);
		const res = await call('DELETE', `tasks/${taskId}`, undefined, persona);
		expect(res!.status).toBe(403);
	});

	it('returns 401, not 403, for a disabled persona even on a route outside the whitelist', async () => {
		await enableLocalPersona(ctx, 'p-1', ['tasks:read', 'tasks:write']);
		const { disableLocalPersona } = await import('../personas');
		await disableLocalPersona(ctx, 'p-1');
		const res = await call('DELETE', `tasks/${taskId}`, undefined, persona);
		expect(res!.status).toBe(401);
	});

	it('lets a persona delete only its own comment, not the owner\'s', async () => {
		await enableLocalPersona(ctx, 'p-1', ['comments:write']);
		const own = await call('POST', `tasks/${taskId}/comments`, { message: 'mine' }, persona);
		const ownersComment = await dispatchLocal(api, ctx, 'POST', `tasks/${taskId}/comments`, {
			message: 'the owner said this',
		});
		const refused = await call('DELETE', `comments/${ownersComment!.data.data.id}`, undefined, persona);
		expect(refused!.status).toBe(403);
		const allowed = await call('DELETE', `comments/${own!.data.data.id}`, undefined, persona);
		expect(allowed!.status).toBe(200);
	});

	it('does not gate the human user actor at all', async () => {
		const res = await call('GET', `tasks/${taskId}`, undefined, undefined);
		expect(res!.status).toBe(200);
	});
});
