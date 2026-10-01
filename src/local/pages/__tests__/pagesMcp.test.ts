import { memoryDb, nodeSqliteAvailable } from '../../__tests__/nodeDb';
import { createLocalApi } from '../../api';
import { dispatchLocal } from '../../dispatch';
import { handleMcpRequest } from '../../mcp';
import { enableLocalPersona } from '../../personas';
import { migrate } from '../../schema';
import type { LocalContext } from '../../types';

const describeSqlite = nodeSqliteAvailable ? describe : describe.skip;

describeSqlite('pages MCP tools for local personas', () => {
	let ctx: LocalContext;
	const clock = new Date('2026-10-01T10:00:00Z');
	const router = createLocalApi();
	const persona = { kind: 'persona' as const, id: 'p-1', name: 'Analyst' };
	const deps = { personaPrompt: async () => null };
	const rpc = (body: unknown) =>
		handleMcpRequest(router, ctx, JSON.stringify(body), deps);
	const call = async (name: string, args: Record<string, unknown> = {}) => {
		const res = await rpc({
			jsonrpc: '2.0',
			id: 1,
			method: 'tools/call',
			params: { name, arguments: args },
		});
		return JSON.parse(res.body).result as {
			isError?: boolean;
			content: { text: string }[];
		};
	};
	const ok = async (name: string, args: Record<string, unknown> = {}) => {
		const result = await call(name, args);
		if (result.isError) throw new Error(`${name}: ${result.content[0].text}`);
		return result.content[0].text;
	};
	const json = async (name: string, args: Record<string, unknown> = {}) =>
		JSON.parse(await ok(name, args));
	const humanPage = async (body: Record<string, unknown>) =>
		(await dispatchLocal(
			router,
			{ ...ctx, actor: undefined },
			'POST',
			'pages',
			body,
		))!.data.data;

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
			now: () => clock,
			files: {
				url: (key) => `tmgr://x/${key}`,
				read: async () => new Blob(['x']),
				remove: async () => {},
			},
			actor: persona,
		};
		await migrate(ctx.db, clock.toISOString());
		await ctx.db.execute(
			`INSERT INTO personas (uuid, owner_user_id, owner_name, name, description, avatar_file, synced_at, archived_at)
			 VALUES ('p-1', 7, 'Yurij', 'Analyst', NULL, NULL, ?, NULL)`,
			[clock.toISOString()],
		);
		await enableLocalPersona(ctx, 'p-1', [
			'pages:read',
			'pages:write',
			'tasks:read',
		]);
	});

	it('lists the pages tools according to the grant', async () => {
		const names = async () =>
			JSON.parse(
				(await rpc({ jsonrpc: '2.0', id: 1, method: 'tools/list' })).body,
			).result.tools.map((t: any) => t.name);
		const all = await names();
		for (const name of [
			'workspace_context',
			'pages_for_task',
			'pages_search',
			'pages_tree',
			'pages_get',
			'pages_create',
			'pages_update',
			'pages_append',
			'pages_set_section',
		]) {
			expect(all).toContain(name);
		}
		await enableLocalPersona(ctx, 'p-1', ['pages:read']);
		const readOnly = await names();
		expect(readOnly).toContain('pages_get');
		expect(readOnly).not.toContain('pages_create');
		await enableLocalPersona(ctx, 'p-1', ['tasks:read']);
		expect(
			(await names()).filter(
				(n: string) => n.startsWith('pages_') || n === 'workspace_context',
			),
		).toEqual([]);
	});

	it('keeps the server tool descriptions and agent rules', async () => {
		const tools = JSON.parse(
			(await rpc({ jsonrpc: '2.0', id: 1, method: 'tools/list' })).body,
		).result.tools;
		const byName = Object.fromEntries(tools.map((t: any) => [t.name, t]));
		expect(byName.workspace_context.description).toContain(
			'AGENT RULES: call workspace_context and pages_for_task before working on a task',
		);
		expect(byName.pages_update.description).toContain(
			'never overwrite someone else',
		);
		expect(byName.pages_append.description).toContain('«Заметки агентов»');
		expect(byName.pages_update.inputSchema.required).toEqual(['id', 'version']);
	});

	it('creates, reads, searches and lists pages with camelCase fields and untouched properties', async () => {
		const created = await json('pages_create', {
			title: 'Notes',
			body: 'quokka facts',
		});
		expect(created).toMatchObject({
			title: 'Notes',
			slug: 'notes',
			version: 1,
			author: { kind: 'persona', id: 'p-1' },
		});
		const person = await json('pages_create', {
			title: 'Ivan',
			type: 'person',
			properties: { company: 'ACME' },
		});
		expect(person.properties).toEqual({
			user_id: null,
			aliases: [],
			network: null,
			company: 'ACME',
			last_contact_at: null,
			role: null,
		});
		expect((await json('pages_get', { id: 'notes' })).id).toBe(created.id);
		const hits = await json('pages_search', { query: 'quokka' });
		expect(hits.total).toBe(1);
		expect(hits.items[0]).toMatchObject({ slug: 'notes', title: 'Notes' });
		const tree = await json('pages_tree');
		expect(tree.items.map((p: any) => p.title).sort()).toEqual([
			'Ivan',
			'Notes',
		]);
		expect(tree.items[0]).toHaveProperty('parentId');
	});

	it('updates with a version and explains a conflict the way the cloud tool does', async () => {
		const page = await json('pages_create', { title: 'Doc', body: 'one' });
		const updated = await json('pages_update', {
			id: page.id,
			version: 1,
			body: 'two',
			summary: 'edit',
		});
		expect(updated.version).toBe(2);
		const stale = await call('pages_update', {
			id: String(page.id),
			version: 1,
			body: 'mine',
		});
		expect(stale.isError).toBe(true);
		expect(stale.content[0].text).toBe(
			'Page version conflict (409 page_conflict): the page is now at version 2. Re-read it with pages_get, merge your change into the current body and retry with that version; never overwrite.',
		);
		const missing = await call('pages_update', { id: page.id, body: 'x' });
		expect(missing.isError).toBe(true);
		expect(missing.content[0].text).toBe('version is required');
	});

	it('appends, writes the agent section of a context page and refuses other sections', async () => {
		const context = await humanPage({ title: 'Контекст', type: 'context' });
		const appended = await json('pages_append', {
			id: context.id,
			markdown: 'did the thing',
			summary: 'session',
		});
		expect(appended.body).toContain(
			'## Заметки агентов\n\ndid the thing\n<!-- /tmgr:section -->',
		);
		const set = await json('pages_set_section', {
			id: context.slug,
			sectionId: 'agent-notes',
			markdown: 'fresh',
		});
		expect(set.sections).toEqual([
			{ id: 'agent-notes', owner: 'agents', heading: 'Заметки агентов' },
		]);
		const person = await humanPage({ title: 'Ivan', type: 'person' });
		const forbidden = await call('pages_set_section', {
			id: person.id,
			sectionId: 'promises',
			markdown: 'x',
		});
		expect(forbidden.isError).toBe(true);
		expect(forbidden.content[0].text).toBe(
			"Section 'promises' is managed by system (403 section_forbidden)",
		);
		const notFound = await call('pages_get', { id: 'ghost' });
		expect(notFound.content[0].text).toBe(
			'Page not found (404 page_not_found)',
		);
	});

	it('returns the workspace context as markdown and the pages mentioning a task', async () => {
		await humanPage({ title: 'Контекст', type: 'context' });
		const text = await ok('workspace_context');
		expect(text.startsWith('# Контекст\n\n*Page `kontekst`')).toBe(true);
		expect(text).toContain('## Как мы работаем');
		const task = (await dispatchLocal(
			router,
			{ ...ctx, actor: undefined },
			'POST',
			'tasks',
			{ title: 'Do it' },
		))!.data.data;
		const page = await humanPage({
			title: 'Plan',
			body: `for [t](tmgr://task/${task.id})`,
		});
		for (const taskId of ['1/../../x', 1.5, -2, 'abc'])
			expect((await call('pages_for_task', { taskId })).isError).toBe(true);
		const found = await json('pages_for_task', { taskId: task.id });
		expect(found).toMatchObject({
			total: 1,
			items: [{ id: page.id, slug: 'plan' }],
		});
	});

	it('refuses a workspaceId of another workspace', async () => {
		const result = await call('pages_tree', { workspaceId: 5 });
		expect(result.isError).toBe(true);
		expect(result.content[0].text).toContain('workspaceId does not match');
	});

	it('is refused with the permission message when the grant lacks it', async () => {
		await enableLocalPersona(ctx, 'p-1', ['pages:read']);
		const result = await call('pages_create', { title: 'x' });
		expect(result.isError).toBe(true);
	});
});
