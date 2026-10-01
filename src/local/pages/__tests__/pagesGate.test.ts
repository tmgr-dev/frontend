import { createLocalApi } from '../../api';
import { dispatchLocal } from '../../dispatch';
import { disableLocalPersona, enableLocalPersona } from '../../personas';
import { PERSONA_WHITELIST } from '../../personaGate';
import { migrate } from '../../schema';
import type { LocalActor, LocalContext } from '../../types';
import { memoryDb, nodeSqliteAvailable } from '../../__tests__/nodeDb';

const describeSqlite = nodeSqliteAvailable ? describe : describe.skip;

describe('pages entries of the persona whitelist', () => {
	const api = createLocalApi();
	const entries = PERSONA_WHITELIST.filter((e) => e.permission.startsWith('pages:'));

	it('use exactly the patterns the router registers', () => {
		const samples: Record<string, string> = {
			'workspaces/context': 'workspaces/context',
			pages: 'pages',
			'pages/tree': 'pages/tree',
			'pages/search': 'pages/search',
			'pages/:id': 'pages/12',
			'pages/:id/backlinks': 'pages/12/backlinks',
			'pages/:id/versions': 'pages/12/versions',
			'pages/:id/versions/:version(\\d+)': 'pages/12/versions/3',
			'pages/:id/files': 'pages/12/files',
			'tasks/:id(\\d+)/pages': 'tasks/4/pages',
			'pages/:id/append': 'pages/12/append',
			'pages/:id/sections/:sectionId': 'pages/12/sections/s',
		};
		expect(entries.length).toBeGreaterThanOrEqual(15);
		for (const entry of entries) {
			const path = samples[entry.pattern];
			expect(path).toBeDefined();
			expect(api.match(entry.method, path)?.pattern).toBe(entry.pattern);
		}
	});

	it('gives reads pages:read and writes pages:write', () => {
		for (const entry of entries) {
			expect(entry.permission).toBe(entry.method === 'GET' ? 'pages:read' : 'pages:write');
		}
	});

	it('leaves tree edits, deletion, restore and conversions to people', () => {
		const open = new Set(entries.map((e) => `${e.method} ${e.pattern}`));
		for (const route of [
			'GET pages/trash',
			'POST pages/:id/move',
			'POST pages/:id/pin',
			'POST pages/:id/unpin',
			'DELETE pages/:id',
			'POST pages/:id/restore',
			'POST pages/:id/versions/:version(\\d+)/restore',
			'POST pages/:id/task-from-selection',
			'POST pages/:id/follow',
		]) {
			expect(open.has(route)).toBe(false);
		}
	});
});

describeSqlite('persona gate on pages routes', () => {
	let ctx: LocalContext;
	let pageId: number;
	const clock = new Date('2026-10-01T10:00:00Z');
	const api = createLocalApi();
	const persona: LocalActor = { kind: 'persona', id: 'p-1', name: 'Analyst' };
	const call = (method: string, url: string, body?: unknown) =>
		dispatchLocal(api, { ...ctx, actor: persona }, method, url, body);

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
			 VALUES ('p-1', 7, 'Yurij', 'Analyst', NULL, NULL, ?, NULL)`,
			[clock.toISOString()],
		);
		const created = await dispatchLocal(api, ctx, 'POST', 'pages', { title: 'Doc', body: 'hello' });
		pageId = created!.data.data.id;
	});

	it('answers reads with pages:read and refuses them without it', async () => {
		await enableLocalPersona(ctx, 'p-1', ['pages:read']);
		for (const url of ['pages', 'pages/tree', 'pages/search?q=hello', `pages/${pageId}`, `pages/${pageId}/versions`, `pages/${pageId}/backlinks`, 'workspaces/context']) {
			expect((await call('GET', url))!.status).toBe(200);
		}
		await enableLocalPersona(ctx, 'p-1', ['pages:write']);
		const res = await call('GET', `pages/${pageId}`);
		expect([res!.status, res!.data.code]).toEqual([403, 'PERMISSION_MISSING']);
	});

	it('answers writes with pages:write and refuses them with read only', async () => {
		await enableLocalPersona(ctx, 'p-1', ['pages:read']);
		const refused = await call('POST', `pages/${pageId}/append`, { markdown: 'x' });
		expect([refused!.status, refused!.data.code]).toEqual([403, 'PERMISSION_MISSING']);
		await enableLocalPersona(ctx, 'p-1', ['pages:read', 'pages:write']);
		expect((await call('POST', `pages/${pageId}/append`, { markdown: 'x' }))!.status).toBe(200);
		expect((await call('POST', 'pages', { title: 'Mine' }))!.status).toBe(201);
	});

	it('refuses routes left to people with ROUTE_NOT_ALLOWED whatever the grant', async () => {
		await enableLocalPersona(ctx, 'p-1', ['pages:read', 'pages:write', 'tasks:write']);
		for (const [method, url] of [
			['GET', 'pages/trash'],
			['POST', `pages/${pageId}/move`],
			['POST', `pages/${pageId}/pin`],
			['DELETE', `pages/${pageId}`],
			['POST', `pages/${pageId}/restore`],
			['POST', `pages/${pageId}/task-from-selection`],
			['POST', `pages/${pageId}/follow`],
			['POST', `pages/${pageId}/versions/1/restore`],
		]) {
			const res = await call(method, url, {});
			expect([res!.status, res!.data.code]).toEqual([403, 'ROUTE_NOT_ALLOWED']);
		}
	});

	it('answers 401 for a disabled persona and keeps its writes attributed to it otherwise', async () => {
		await enableLocalPersona(ctx, 'p-1', ['pages:read', 'pages:write']);
		const written = await call('POST', `pages/${pageId}/append`, { markdown: 'by persona', summary: 'note' });
		expect(written!.data.data.updated_by).toMatchObject({ kind: 'persona', id: 'p-1', name: 'Analyst' });
		const [row] = await ctx.db.select<any>(`SELECT author_kind, author_ref FROM page_versions WHERE page_id = ? AND version = 2`, [pageId]);
		expect(row).toEqual({ author_kind: 'persona', author_ref: 'p-1' });
		await disableLocalPersona(ctx, 'p-1');
		expect((await call('GET', `pages/${pageId}`))!.status).toBe(401);
	});

	it('journals persona page writes and refusals in the activity log', async () => {
		await enableLocalPersona(ctx, 'p-1', ['pages:read', 'pages:write']);
		await call('POST', `pages/${pageId}/append`, { markdown: 'x' });
		await call('DELETE', `pages/${pageId}`);
		const rows = await ctx.db.select<any>(`SELECT method, route, status, entity FROM activity_log ORDER BY id`);
		expect(rows).toEqual([
			{ method: 'POST', route: 'pages/:id/append', status: 200, entity: 'pages' },
			{ method: 'DELETE', route: 'pages/:id', status: 403, entity: 'pages' },
		]);
	});
});
