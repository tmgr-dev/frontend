import { domainEvents, type DomainEvent } from '@/utils/domainEvents';
import { memoryDb, nodeSqliteAvailable } from '../../__tests__/nodeDb';
import { createLocalApi } from '../../api';
import { dispatchLocal } from '../../dispatch';
import { enableLocalPersona } from '../../personas';
import { migrate } from '../../schema';
import type { LocalActor, LocalContext } from '../../types';
import { ensureContextPage } from '../service';

const describeSqlite = nodeSqliteAvailable ? describe : describe.skip;

describeSqlite('local pages API on SQLite', () => {
	let ctx: LocalContext;
	let clock: Date;
	const api = createLocalApi();
	const persona: LocalActor = { kind: 'persona', id: 'p-1', name: 'Analyst' };
	const plugin: LocalActor = {
		kind: 'plugin',
		id: 'tmgr.people',
		name: 'People',
	};

	const tick = () => {
		clock = new Date(clock.getTime() + 1000);
	};
	const call = async (
		method: string,
		url: string,
		body?: unknown,
		actor?: LocalActor,
	) => {
		tick();
		const res = await dispatchLocal(api, { ...ctx, actor }, method, url, body);
		if (!res) throw new Error(`no route ${method} ${url}`);
		return res;
	};
	const data = async (
		method: string,
		url: string,
		body?: unknown,
		actor?: LocalActor,
	) => {
		const res = await call(method, url, body, actor);
		if (res.status >= 400)
			throw new Error(
				`${method} ${url} -> ${res.status} ${JSON.stringify(res.data)}`,
			);
		return res.data.data;
	};
	const create = (
		title: string,
		extra: Record<string, unknown> = {},
		actor?: LocalActor,
	) => data('POST', 'pages', { title, ...extra }, actor);

	beforeEach(async () => {
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
			user: { id: 7, name: 'Yurij', email: 'me@example.com' },
			now: () => clock,
			files: {
				url: (key) => `tmgrfile://localhost/${key}`,
				read: async () => new Blob(['x']),
				remove: async () => {},
			},
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

	describe('create and read', () => {
		it('creates a plain page with an empty body, version 1 and the author', async () => {
			const page = await create('Hello world');
			expect(page).toMatchObject({
				title: 'Hello world',
				slug: 'hello-world',
				type: 'plain',
				parent_id: null,
				body: '',
				properties: {},
				version: 1,
				pinned: false,
				workspace_id: -1,
				author: { kind: 'user', id: 7, name: 'Yurij' },
				updated_by: { kind: 'user', id: 7 },
				backlinks: [],
				sections: [],
				files: [],
			});
			expect((await data('GET', 'pages/hello-world')).id).toBe(page.id);
			expect((await data('GET', `pages/${page.id}`)).slug).toBe('hello-world');
			const versions = await data('GET', `pages/${page.id}/versions`);
			expect(versions.map((v: any) => v.version)).toEqual([1]);
		});

		it('answers 201 and rejects a blank or oversized title', async () => {
			expect((await call('POST', 'pages', { title: 'A' })).status).toBe(201);
			const blank = await call('POST', 'pages', { title: '  ' });
			expect([blank.status, blank.data.error]).toEqual([422, 'title_required']);
			const long = await call('POST', 'pages', { title: 'x'.repeat(256) });
			expect([long.status, long.data.error]).toEqual([422, 'title_too_long']);
			const unknown = await call('POST', 'pages', { title: 'A', type: 'wiki' });
			expect([unknown.status, unknown.data.error]).toEqual([
				422,
				'unsupported_type',
			]);
		});

		it('transliterates slugs, suffixes collisions and prefixes numeric or reserved slugs', async () => {
			expect((await create('Привет, мир')).slug).toBe('privet-mir');
			const second = await create('Привет мир');
			expect(second.slug).toMatch(/^privet-mir-[a-z0-9]{4}$/);
			expect((await create('2026')).slug).toBe('p-2026');
			expect((await create('Tree')).slug).toBe('p-tree');
			expect((await create('!!!')).slug).toBe('page');
		});

		it('keeps the slug when the title changes', async () => {
			const page = await create('First');
			const renamed = await data('PATCH', `pages/${page.id}`, {
				version: 1,
				title: 'Second',
			});
			expect([renamed.slug, renamed.title]).toEqual(['first', 'Second']);
		});

		it('nests pages and lists roots, children, tree and filters by type', async () => {
			const root = await create('Root');
			const child = await create('Child', { parent_id: root.id });
			await create('Other', { type: 'meeting' });
			expect((await data('GET', 'pages')).map((p: any) => p.title)).toEqual([
				'Root',
				'Other',
			]);
			expect(
				(await data('GET', `pages?parent_id=${root.id}`)).map((p: any) => p.id),
			).toEqual([child.id]);
			expect(
				(await data('GET', 'pages?type=meeting')).map((p: any) => p.title),
			).toEqual(['Other']);
			expect(
				(await data('GET', 'pages/tree')).map((p: any) => p.title).sort(),
			).toEqual(['Child', 'Other', 'Root']);
			const missing = await call('POST', 'pages', {
				title: 'Orphan',
				parent_id: 999,
			});
			expect([missing.status, missing.data.error]).toEqual([
				422,
				'parent_not_found',
			]);
		});

		it('returns 404 for an unknown page by id or slug', async () => {
			expect((await call('GET', 'pages/999')).status).toBe(404);
			const res = await call('GET', 'pages/nope');
			expect([res.status, res.data.error]).toEqual([404, 'page_not_found']);
		});
	});

	describe('page types', () => {
		it('creates a context page from the template, pinned, with the agent section', async () => {
			const page = await create('Контекст', { type: 'context' });
			expect(page.pinned).toBe(true);
			expect(page.sections).toEqual([
				{ id: 'agent-notes', owner: 'agents', heading: 'Заметки агентов' },
			]);
			expect(page.body).toContain('## Как мы работаем');
		});

		it('creates person and meeting pages from their templates', async () => {
			const person = await create('Иван', { type: 'person' });
			expect(person.sections).toEqual([
				{ id: 'promises', owner: 'system', heading: 'Обещания' },
			]);
			expect(person.body).toContain('## Хронология');
			expect(person.properties).toEqual({
				user_id: null,
				aliases: [],
				network: null,
				company: null,
				role: null,
				last_contact_at: null,
			});
			const meeting = await create('Sync', { type: 'meeting' });
			expect(meeting.body).toContain('## Повестка');
			expect(meeting.properties).toEqual({
				date: null,
				participants: [],
				related_tasks: [],
			});
		});

		it('validates person and meeting properties with field errors', async () => {
			const person = await call('POST', 'pages', {
				title: 'P',
				type: 'person',
				properties: {
					network: 'x',
					aliases: [{ source: 'icq', native_id: '' }],
					bogus: 1,
					user_id: 'a',
				},
			});
			expect([person.status, person.data.error]).toEqual([
				422,
				'invalid_properties',
			]);
			expect(Object.keys(person.data.errors).sort()).toEqual(
				[
					'aliases[0].native_id',
					'aliases[0].source',
					'bogus',
					'network',
					'user_id',
				].sort(),
			);
			const meeting = await call('POST', 'pages', {
				title: 'M',
				type: 'meeting',
				properties: {
					date: '2026-02-31',
					participants: ['x'],
					related_tasks: [0],
				},
			});
			expect(Object.keys(meeting.data.errors).sort()).toEqual([
				'date',
				'participants[0]',
				'related_tasks[0]',
			]);
			const ok = await create('M2', {
				type: 'meeting',
				properties: {
					date: '2026-10-01',
					participants: ['tmgr://user/7'],
					related_tasks: [3],
				},
			});
			expect(ok.properties.date).toBe('2026-10-01');
		});

		it('ignores a client last_contact_at and keeps the stored one on update', async () => {
			const person = await create('Иван', {
				type: 'person',
				properties: { last_contact_at: '1999-01-01', company: 'ACME' },
			});
			expect(person.properties.last_contact_at).toBeNull();
			const updated = await data('PATCH', `pages/${person.id}`, {
				version: 1,
				properties: { company: 'Beta', last_contact_at: '1999-01-01' },
			});
			expect(updated.properties).toMatchObject({
				company: 'Beta',
				last_contact_at: null,
			});
		});

		it('lets the owner pick an analyst persona who then owns the insights section of new person pages', async () => {
			expect(await data('GET', 'workspaces/-1/settings')).toEqual({
				'pages.analyst_persona_id': null,
			});
			const before = await create('Иван', { type: 'person' });
			expect(before.sections.map((s: any) => s.id)).toEqual(['promises']);
			const saved = await data('PUT', 'workspaces/-1/settings', {
				settings: { 'pages.analyst_persona_id': 'p-1' },
			});
			expect(saved).toEqual({ 'pages.analyst_persona_id': 'p-1' });
			const after = await create('Анна', { type: 'person' });
			expect(after.sections).toEqual([
				{ id: 'promises', owner: 'system', heading: 'Обещания' },
				{ id: 'insights', owner: 'persona:p-1', heading: 'Инсайты' },
			]);
			expect(
				(await data('GET', `pages/${before.id}`)).sections.map(
					(s: any) => s.id,
				),
			).toEqual(['promises']);
			const own = await data(
				'PUT',
				`pages/${after.id}/sections/insights`,
				{ markdown: 'pattern noticed' },
				persona,
			);
			expect(own.body).toContain('pattern noticed');
			await data('PUT', 'workspaces/-1/settings', {
				settings: { 'pages.analyst_persona_id': null },
			});
			expect(await data('GET', 'workspaces/-1/settings')).toEqual({
				'pages.analyst_persona_id': null,
			});
		});

		it('rejects unknown settings, unavailable personas and non-human callers', async () => {
			const unknown = await call('PUT', 'workspaces/-1/settings', {
				settings: { nope: 1 },
			});
			expect([unknown.status, unknown.data.error]).toEqual([
				422,
				'unknown_setting',
			]);
			const ghost = await call('PUT', 'workspaces/-1/settings', {
				settings: { 'pages.analyst_persona_id': 'ghost' },
			});
			expect([ghost.status, ghost.data.error]).toEqual([
				422,
				'invalid_persona',
			]);
			expect(
				(
					await call(
						'PUT',
						'workspaces/-1/settings',
						{ settings: { 'pages.analyst_persona_id': 'p-1' } },
						plugin,
					)
				).status,
			).toBe(403);
			expect(
				(
					await call(
						'PUT',
						'workspaces/-1/settings',
						{ settings: { 'pages.analyst_persona_id': 'p-1' } },
						persona,
					)
				).status,
			).toBe(403);
		});

		it('refuses a persona creating a context page but lets it create a plain one', async () => {
			const res = await call(
				'POST',
				'pages',
				{ title: 'C', type: 'context' },
				persona,
			);
			expect([res.status, res.data.error]).toEqual([403, 'forbidden']);
			const plain = await create('Notes', {}, persona);
			expect(plain.author).toMatchObject({
				kind: 'persona',
				id: 'p-1',
				name: 'Analyst',
				owner: { id: '7', name: 'Yurij' },
			});
		});

		it('does not duplicate the context page when opened twice at once or when one already exists', async () => {
			await Promise.all([ensureContextPage(ctx), ensureContextPage(ctx)]);
			expect(await data('GET', 'pages')).toHaveLength(1);
			const other = memoryDb();
			const second: LocalContext = { ...ctx, db: other };
			await migrate(other, clock.toISOString());
			await dispatchLocal(api, second, 'POST', 'pages', {
				title: 'Mine',
				type: 'context',
			});
			await ensureContextPage(second);
			expect(
				(await dispatchLocal(api, second, 'GET', 'pages'))!.data.data,
			).toHaveLength(1);
		});

		it('creates the context page once when a workspace is first opened and never again', async () => {
			await ensureContextPage(ctx);
			const pages = await data('GET', 'pages');
			expect(pages).toHaveLength(1);
			expect([pages[0].title, pages[0].type, pages[0].pinned]).toEqual([
				'Контекст воркспейса',
				'context',
				true,
			]);
			await data('DELETE', `pages/${pages[0].id}`);
			await ensureContextPage(ctx);
			expect(await data('GET', 'pages')).toHaveLength(0);
		});
	});

	describe('update and optimistic locking', () => {
		it('requires a version and answers 409 with the current page on a stale one', async () => {
			const page = await create('Doc', { body: 'one' });
			const missing = await call('PATCH', `pages/${page.id}`, { body: 'x' });
			expect([missing.status, missing.data.error]).toEqual([
				422,
				'version_required',
			]);
			await data('PATCH', `pages/${page.id}`, {
				version: 1,
				body: 'two',
				summary: 'second',
			});
			const stale = await call('PATCH', `pages/${page.id}`, {
				version: 1,
				body: 'mine',
			});
			expect(stale.status).toBe(409);
			expect(stale.data).toMatchObject({
				error: 'page_conflict',
				message: 'Page was changed',
				data: { id: page.id, version: 2, body: 'two' },
			});
			const ok = await data('PATCH', `pages/${page.id}`, {
				version: 2,
				body: 'three',
			});
			expect(ok.version).toBe(3);
			const versions = await data('GET', `pages/${page.id}/versions`);
			expect(versions.map((v: any) => [v.version, v.summary])).toEqual([
				[3, null],
				[2, 'second'],
				[1, null],
			]);
			expect((await data('GET', `pages/${page.id}/versions/2`)).body).toBe(
				'two',
			);
			expect((await call('GET', `pages/${page.id}/versions/9`)).status).toBe(
				404,
			);
		});

		it('does not bump the version when nothing changed', async () => {
			const page = await create('Doc', { body: 'same' });
			const again = await data('PATCH', `pages/${page.id}`, {
				version: 1,
				body: 'same',
			});
			expect(again.version).toBe(1);
		});

		it('rejects an oversized body with 413 on create, patch and append, leaving the page untouched', async () => {
			const huge = 'я'.repeat(524_289);
			const tooBig = await call('POST', 'pages', { title: 'Big', body: huge });
			expect([tooBig.status, tooBig.data.error]).toEqual([
				413,
				'page_too_large',
			]);
			const page = await create('Doc', { body: 'x'.repeat(1_048_000) });
			expect(
				(await call('PATCH', `pages/${page.id}`, { version: 1, body: huge }))
					.status,
			).toBe(413);
			const append = await call('POST', `pages/${page.id}/append`, {
				markdown: 'y'.repeat(2000),
			});
			expect(append.status).toBe(413);
			expect((await data('GET', `pages/${page.id}`)).version).toBe(1);
		});

		it('restores a version as a new version and keeps the old ones; only people may', async () => {
			const page = await create('Doc', { body: 'one' });
			await data('PATCH', `pages/${page.id}`, { version: 1, body: 'two' });
			const restored = await data(
				'POST',
				`pages/${page.id}/versions/1/restore`,
			);
			expect([restored.version, restored.body]).toEqual([3, 'one']);
			expect(
				(await data('GET', `pages/${page.id}/versions`)).map(
					(v: any) => v.version,
				),
			).toEqual([3, 2, 1]);
			expect((await data('GET', `pages/${page.id}/versions`))[0].summary).toBe(
				'Restored version 1',
			);
			expect(
				(
					await call(
						'POST',
						`pages/${page.id}/versions/1/restore`,
						undefined,
						persona,
					)
				).status,
			).toBe(403);
		});

		const racingWindow = (times: number): LocalContext => {
			const real = ctx.db;
			let left = times;
			return {
				...ctx,
				db: {
					select: real.select.bind(real),
					execute: real.execute.bind(real),
					async batch(statements) {
						if (
							left > 0 &&
							statements[0].sql.startsWith('UPDATE pages SET title')
						) {
							left--;
							await real.execute(
								`UPDATE pages SET version = version + 1, body = body || ' (other window)' WHERE id = 1`,
							);
						}
						return real.batch(statements);
					},
				},
			};
		};

		it('replans from the page another window changed in between and keeps both edits', async () => {
			const page = await create('Doc', { body: 'start' });
			const res = await dispatchLocal(
				api,
				racingWindow(1),
				'POST',
				`pages/${page.id}/append`,
				{ markdown: 'mine' },
			);
			expect(res!.status).toBe(200);
			expect(res!.data.data).toMatchObject({
				version: 3,
				body: 'start (other window)\n\nmine\n',
			});
		});

		it('answers 409 with the current page and writes nothing when the page keeps changing', async () => {
			const task = await data('POST', 'tasks', { title: 'T' });
			const page = await create('Doc', { body: 'start' });
			const res = await dispatchLocal(
				api,
				racingWindow(3),
				'POST',
				`pages/${page.id}/append`,
				{ markdown: `see [t](tmgr://task/${task.id})` },
			);
			expect(res!.status).toBe(409);
			expect(res!.data).toMatchObject({
				error: 'page_conflict',
				data: { version: 4 },
			});
			expect(
				await ctx.db.select(
					`SELECT version FROM page_versions WHERE page_id = ?`,
					[page.id],
				),
			).toEqual([{ version: 1 }]);
			expect(
				await ctx.db.select(`SELECT * FROM page_links WHERE page_id = ?`, [
					page.id,
				]),
			).toEqual([]);
		});

		it('survives ten parallel appends with every line present', async () => {
			const page = await create('Log', { body: '## Log\n' });
			await Promise.all(
				Array.from({ length: 10 }, (_, i) =>
					call('POST', `pages/${page.id}/append`, { markdown: `- line ${i}` }),
				),
			);
			const final = await data('GET', `pages/${page.id}`);
			expect(final.version).toBe(11);
			for (let i = 0; i < 10; i++)
				expect(final.body).toContain(`- line ${i}\n`);
		});
	});

	describe('append', () => {
		it('appends to the end, under a heading and creates a missing heading on request', async () => {
			const page = await create('Doc', {
				body: '## A\n\ntext\n\n## B\n\nmore\n',
			});
			let out = await data('POST', `pages/${page.id}/append`, {
				markdown: 'tail',
			});
			expect(out.body.endsWith('more\n\ntail\n')).toBe(true);
			out = await data('POST', `pages/${page.id}/append`, {
				markdown: 'under a',
				heading: 'a',
			});
			expect(out.body).toContain('## A\n\ntext\n\nunder a\n\n## B');
			const missing = await call('POST', `pages/${page.id}/append`, {
				markdown: 'x',
				heading: 'Nope',
			});
			expect([missing.status, missing.data.error]).toEqual([
				422,
				'heading_not_found',
			]);
			out = await data('POST', `pages/${page.id}/append`, {
				markdown: '- one',
				heading: 'Nope',
				create_heading: true,
				summary: 'added',
			});
			expect(out.body.endsWith('## Nope\n\n- one\n')).toBe(true);
			expect((await data('GET', `pages/${page.id}/versions`))[0].summary).toBe(
				'added',
			);
			const empty = await call('POST', `pages/${page.id}/append`, {
				markdown: ' ',
			});
			expect([empty.status, empty.data.error]).toEqual([
				422,
				'markdown_required',
			]);
		});

		it('bumps last_contact_at when a person page gets a Хронология entry', async () => {
			const person = await create('Иван', { type: 'person' });
			const out = await data('POST', `pages/${person.id}/append`, {
				markdown: '- звонили',
				heading: 'Хронология',
			});
			expect(out.properties.last_contact_at).toBe('2026-10-01');
			expect(out.body).toContain('## Хронология\n\n- звонили');
		});

		it('lets a persona append to the agent section of a context page, but not elsewhere', async () => {
			const context = await create('Контекст', { type: 'context' });
			const out = await data(
				'POST',
				`pages/${context.id}/append`,
				{ markdown: 'session summary' },
				persona,
			);
			expect(out.body).toContain(
				'## Заметки агентов\n\nsession summary\n<!-- /tmgr:section -->',
			);
			expect(out.updated_by).toMatchObject({ kind: 'persona', id: 'p-1' });
			const outside = await call(
				'POST',
				`pages/${context.id}/append`,
				{ markdown: 'x', heading: 'Архитектура' },
				persona,
			);
			expect([outside.status, outside.data.error]).toEqual([
				403,
				'section_forbidden',
			]);
		});

		it('refuses section markers in text written by a persona or a plugin', async () => {
			const page = await create('Doc', { body: 'x' });
			const forged =
				'<!-- tmgr:section id="evil" owner="agents" -->\nx\n<!-- /tmgr:section -->';
			for (const actor of [persona, plugin]) {
				const res = await call(
					'POST',
					`pages/${page.id}/append`,
					{ markdown: forged },
					actor,
				);
				expect([res.status, res.data.error]).toEqual([
					403,
					'section_forbidden',
				]);
			}
			expect(
				(await call('POST', `pages/${page.id}/append`, { markdown: forged }))
					.status,
			).toBe(200);
		});
	});

	describe('sections', () => {
		const managed = (owner: string) =>
			`intro\n\n<!-- tmgr:section id="mine" owner="${owner}" -->\n## Insights\n\nold\n<!-- /tmgr:section -->\n\n<!-- tmgr:section id="sys" owner="system" -->\n## Promises\n<!-- /tmgr:section -->\n`;

		it('lets a human write any section and keeps the section heading', async () => {
			const page = await create('Doc', { body: managed('persona:p-1') });
			const out = await data('PUT', `pages/${page.id}/sections/sys`, {
				markdown: '- item',
				summary: 'hand edit',
			});
			expect(out.body).toContain(
				'## Promises\n\n- item\n<!-- /tmgr:section -->',
			);
			expect(out.version).toBe(2);
		});

		it('lets the owning persona write its section but not another persona or system', async () => {
			const page = await create('Doc', { body: managed('persona:p-1') });
			const own = await data(
				'PUT',
				`pages/${page.id}/sections/mine`,
				{ markdown: 'fresh' },
				persona,
			);
			expect(own.body).toContain('## Insights\n\nfresh\n');
			expect(own.updated_by).toMatchObject({ kind: 'persona', id: 'p-1' });
			const sys = await call(
				'PUT',
				`pages/${page.id}/sections/sys`,
				{ markdown: 'hack' },
				persona,
			);
			expect([sys.status, sys.data.error]).toEqual([403, 'section_forbidden']);
			const other = await create('Doc 2', { body: managed('persona:p-2') });
			expect(
				(
					await call(
						'PUT',
						`pages/${other.id}/sections/mine`,
						{ markdown: 'x' },
						persona,
					)
				).status,
			).toBe(403);
			expect(
				(
					await call(
						'PUT',
						`pages/${page.id}/sections/zzz`,
						{ markdown: 'x' },
						persona,
					)
				).status,
			).toBe(404);
		});

		it('lets a plugin write only the section owned as plugin:<id>', async () => {
			const page = await create('Doc', { body: managed('plugin:tmgr.people') });
			expect(
				(
					await data(
						'PUT',
						`pages/${page.id}/sections/mine`,
						{ markdown: 'plugin text' },
						plugin,
					)
				).body,
			).toContain('plugin text');
			const other = await create('Doc 2', { body: managed('plugin:other') });
			expect(
				(
					await call(
						'PUT',
						`pages/${other.id}/sections/mine`,
						{ markdown: 'x' },
						plugin,
					)
				).status,
			).toBe(403);
		});

		it('allows only agents sections on a context page', async () => {
			const context = await create('Контекст', { type: 'context' });
			const ok = await data(
				'PUT',
				`pages/${context.id}/sections/agent-notes`,
				{ markdown: 'note' },
				persona,
			);
			expect(ok.body).toContain('note');
			const patched = await call(
				'PATCH',
				`pages/${context.id}`,
				{ version: ok.version, title: 'Renamed' },
				persona,
			);
			expect([patched.status, patched.data.error]).toEqual([
				403,
				'section_forbidden',
			]);
			const hijack = await call(
				'PATCH',
				`pages/${context.id}`,
				{
					version: ok.version,
					body: ok.body.replace('## Архитектура', '## Архитектура\n\nhijack'),
				},
				persona,
			);
			expect(hijack.status).toBe(403);
		});

		it('refuses a PATCH from a persona that touches a managed section or its markers', async () => {
			const page = await create('Doc', { body: managed('persona:p-1') });
			const touched = await call(
				'PATCH',
				`pages/${page.id}`,
				{
					version: 1,
					body: page.body.replace('## Promises', '## Promises\nhack'),
				},
				persona,
			);
			expect(touched.status).toBe(403);
			const dropped = await call(
				'PATCH',
				`pages/${page.id}`,
				{ version: 1, body: 'intro' },
				persona,
			);
			expect(dropped.status).toBe(403);
			const ownEdit = await call(
				'PATCH',
				`pages/${page.id}`,
				{ version: 1, body: page.body.replace('old', 'new') },
				persona,
			);
			expect(ownEdit.status).toBe(200);
		});

		it('rejects duplicate, nested and unbalanced markers with 422', async () => {
			const dup =
				'<!-- tmgr:section id="a" owner="agents" -->\nx\n<!-- /tmgr:section -->\n<!-- tmgr:section id="a" owner="agents" -->\ny\n<!-- /tmgr:section -->';
			const nested =
				'<!-- tmgr:section id="a" owner="agents" -->\n<!-- tmgr:section id="b" owner="agents" -->\n<!-- /tmgr:section -->';
			const stray = 'x\n<!-- /tmgr:section -->';
			for (const body of [dup, nested, stray]) {
				const res = await call('POST', 'pages', { title: 'Bad', body });
				expect([res.status, res.data.error]).toEqual([422, 'invalid_sections']);
			}
		});
	});

	describe('tree operations', () => {
		const titles = async (parent: number | null) =>
			(
				await data(
					'GET',
					parent === null ? 'pages' : `pages?parent_id=${parent}`,
				)
			).map((p: any) => p.title);

		it('moves with index semantics and renumbers siblings', async () => {
			const a = await create('A');
			const b = await create('B');
			const c = await create('C');
			await data('POST', `pages/${c.id}/move`, {
				parent_id: null,
				position: 0,
			});
			expect(await titles(null)).toEqual(['C', 'A', 'B']);
			await data('POST', `pages/${c.id}/move`, {
				parent_id: a.id,
				position: 5,
			});
			expect(await titles(null)).toEqual(['A', 'B']);
			expect(await titles(a.id)).toEqual(['C']);
			expect((await data('GET', `pages/${c.id}`)).position).toBe(0);
			const moved = await data('POST', `pages/${b.id}/move`, {
				parent_id: a.id,
				position: 0,
			});
			expect(moved.parent_id).toBe(a.id);
			expect(await titles(a.id)).toEqual(['B', 'C']);
		});

		it('refuses cycles and missing parents; personas cannot move, pin, delete or restore', async () => {
			const a = await create('A');
			const b = await create('B', { parent_id: a.id });
			const c = await create('C', { parent_id: b.id });
			for (const parent of [a.id, c.id, b.id]) {
				const res = await call('POST', `pages/${a.id}/move`, {
					parent_id: parent,
					position: 0,
				});
				expect([res.status, res.data.error]).toEqual([422, 'page_cycle']);
			}
			expect(
				(await call('POST', `pages/${a.id}/move`, { parent_id: 999 })).data
					.error,
			).toBe('parent_not_found');
			for (const [method, url] of [
				['POST', `pages/${a.id}/move`],
				['POST', `pages/${a.id}/pin`],
				['DELETE', `pages/${a.id}`],
				['POST', `pages/${a.id}/restore`],
			]) {
				expect((await call(method, url, {}, persona)).status).toBe(403);
			}
		});

		it('pins and unpins, and pinned pages sort first', async () => {
			const a = await create('A');
			const b = await create('B');
			expect((await data('POST', `pages/${b.id}/pin`)).pinned).toBe(true);
			expect(await titles(null)).toEqual(['B', 'A']);
			expect((await data('POST', `pages/${b.id}/unpin`)).pinned).toBe(false);
			expect(await titles(null)).toEqual(['A', 'B']);
			expect(a.pinned).toBe(false);
		});

		it('deletes a subtree, lists only its top in the trash and restores it together', async () => {
			const root = await create('Root');
			const child = await create('Child', { parent_id: root.id });
			const grand = await create('Grand', { parent_id: child.id });
			const solo = await create('Solo');
			const removed = await data('DELETE', `pages/${root.id}`);
			expect(removed).toEqual({ deleted: 3 });
			expect(await titles(null)).toEqual(['Solo']);
			expect((await call('GET', `pages/${grand.id}`)).status).toBe(404);
			const trash = await data('GET', 'pages/trash');
			expect(trash.map((p: any) => p.id)).toEqual([root.id]);
			expect(trash[0].deleted_at).toBeTruthy();
			const restored = await data('POST', `pages/${root.id}/restore`);
			expect(restored.deleted_at).toBeNull();
			expect(
				(await data('GET', 'pages/tree')).map((p: any) => p.id).sort(),
			).toEqual([root.id, child.id, grand.id, solo.id].sort());
			expect((await call('POST', `pages/${root.id}/restore`)).data.error).toBe(
				'page_not_deleted',
			);
		});

		it('restores a page whose parent is still deleted to the root and keeps earlier deletions separate', async () => {
			const parent = await create('Parent');
			const child = await create('Child', { parent_id: parent.id });
			await data('DELETE', `pages/${child.id}`);
			await data('DELETE', `pages/${parent.id}`);
			const trash = await data('GET', 'pages/trash');
			expect(trash.map((p: any) => p.title).sort()).toEqual([
				'Child',
				'Parent',
			]);
			const restored = await data('POST', `pages/${child.id}/restore`);
			expect(restored.parent_id).toBeNull();
			expect(
				(await data('GET', 'pages/trash')).map((p: any) => p.title),
			).toEqual(['Parent']);
			await data('POST', `pages/${parent.id}/restore`);
			expect(await data('GET', `pages?parent_id=${parent.id}`)).toEqual([]);
		});
	});

	describe('links and mentions', () => {
		const makeTask = async (
			title: string,
			extra: Record<string, unknown> = {},
		) => data('POST', 'tasks', { title, ...extra });

		it('turns task keys into links, extracts tmgr links into backlinks and hides deleted linkers', async () => {
			const category = await data('POST', 'project_categories', {
				title: 'Team',
				code: 'TM',
			});
			const task = await makeTask('Fix login', {
				project_category_id: category.id,
			});
			const key = `${category.code}-${task.category_tasks_sequence_id}`;
			const target = await create('Target');
			const page = await create('Notes', {
				body: `Working on ${key} and TM-999, see [t](tmgr://page/${target.id}).`,
			});
			expect(page.body).toContain(`[${key}](tmgr://task/${task.id})`);
			expect(page.body).toContain('TM-999');
			expect(
				(await data('GET', `pages/${target.id}/backlinks`)).map(
					(p: any) => p.id,
				),
			).toEqual([page.id]);
			expect(
				(await data('GET', `pages/${target.id}`)).backlinks.map(
					(p: any) => p.id,
				),
			).toEqual([page.id]);
			expect(
				(await data('GET', `tasks/${task.id}/pages`)).map((p: any) => p.id),
			).toEqual([page.id]);
			await data('PATCH', `pages/${page.id}`, {
				version: 1,
				body: 'no links now',
			});
			expect(await data('GET', `tasks/${task.id}/pages`)).toEqual([]);
			expect((await call('GET', 'tasks/9999/pages')).data.error).toBe(
				'task_not_found',
			);
		});

		it('keeps task keys in code untouched and links by the title prefix', async () => {
			const task = await makeTask('AB-5: custom prefix');
			const page = await create('Notes', { body: 'See AB-5 and `AB-5`.' });
			expect(page.body).toBe(
				`See [AB-5](tmgr://task/${task.id}) and \`AB-5\`.\n`.replace(/\n$/, ''),
			);
		});

		it('ignores links to targets that do not exist', async () => {
			const page = await create('Notes', {
				body: '[x](tmgr://page/999) [y](tmgr://task/999) [me](tmgr://user/7) [z](tmgr://user/8)',
			});
			const row = await ctx.db.select<any>(
				`SELECT target_kind, target_id FROM page_links WHERE page_id = ?`,
				[page.id],
			);
			expect(row).toEqual([{ target_kind: 'user', target_id: 7 }]);
		});

		it('indexes tasks that mention a page and rebuilds the promises of a person page', async () => {
			const person = await create('Иван', { type: 'person' });
			const status = (await data('GET', 'workspaces/statuses'))?.[0];
			const open = await makeTask('Send the report', {
				description: `Promised to [Иван](tmgr://page/${person.id})`,
				expired_at: '2026-10-05',
			});
			let page = await data('GET', `pages/${person.id}`);
			expect(page.body).toContain(
				`- [T${open.id}](tmgr://task/${open.id}) — Send the report · до 2026-10-05`,
			);
			expect(page.version).toBe(2);
			expect(
				await ctx.db.select(
					`SELECT * FROM task_page_mentions WHERE page_id = ?`,
					[person.id],
				),
			).toHaveLength(1);
			expect((await data('GET', `tasks/${open.id}`)).mentioned_people).toEqual([
				{ page_id: person.id, slug: 'ivan', title: 'Иван' },
			]);

			await data('PATCH', `tasks/${open.id}`, { priority: 'high' });
			page = await data('GET', `pages/${person.id}`);
			expect(page.version).toBe(2);

			const done = (
				await ctx.db.select<any>(
					`SELECT id FROM statuses WHERE type = 'completed'`,
				)
			)[0].id;
			await data('PATCH', `tasks/${open.id}`, { status_id: done });
			page = await data('GET', `pages/${person.id}`);
			expect(page.body).not.toContain('Send the report');

			await data('PATCH', `tasks/${open.id}`, {
				status_id: status?.id ?? 1,
				description: 'no mention',
			});
			expect(
				await ctx.db.select(
					`SELECT * FROM task_page_mentions WHERE page_id = ?`,
					[person.id],
				),
			).toEqual([]);
		});

		it('picks up mentions from comments and drops them when the task is deleted', async () => {
			const person = await create('Анна', { type: 'person' });
			const task = await makeTask('Call back');
			await data('POST', `tasks/${task.id}/comments`, {
				message: `ping [Анна](tmgr://page/${person.id})`,
			});
			expect(
				await ctx.db.select(
					`SELECT * FROM task_page_mentions WHERE task_id = ?`,
					[task.id],
				),
			).toHaveLength(1);
			expect((await data('GET', `pages/${person.id}`)).body).toContain(
				'Call back',
			);
			await data('DELETE', `tasks/${task.id}`);
			expect(
				await ctx.db.select(
					`SELECT * FROM task_page_mentions WHERE task_id = ?`,
					[task.id],
				),
			).toEqual([]);
			expect((await data('GET', `pages/${person.id}`)).body).not.toContain(
				'Call back',
			);
		});
	});

	describe('search', () => {
		it('finds pages by words in the title or body, with a snippet, and hides deleted ones', async () => {
			const alpha = await create('Release plan', {
				body: 'We ship the кузнечик feature next week.',
			});
			await create('Other', { body: 'unrelated text' });
			const hits = await data('GET', 'pages/search?q=кузнеч');
			expect(hits).toHaveLength(1);
			expect(hits[0]).toMatchObject({
				id: alpha.id,
				slug: 'release-plan',
				title: 'Release plan',
				type: 'plain',
			});
			expect(hits[0].snippet).toContain('кузнечик');
			expect(
				(await data('GET', 'pages/search?q=release%20PLAN')).map(
					(h: any) => h.id,
				),
			).toEqual([alpha.id]);
			expect(await data('GET', 'pages/search?q=кузнечик&type=meeting')).toEqual(
				[],
			);
			await data('DELETE', `pages/${alpha.id}`);
			expect(await data('GET', 'pages/search?q=кузнечик')).toEqual([]);
			await data('POST', `pages/${alpha.id}/restore`);
			expect(await data('GET', 'pages/search?q=кузнечик')).toHaveLength(1);
		});

		it('follows edits: new text is found, old text is not', async () => {
			const page = await create('Doc', { body: 'zebra' });
			await data('PATCH', `pages/${page.id}`, { version: 1, body: 'giraffe' });
			expect(await data('GET', 'pages/search?q=zebra')).toEqual([]);
			expect(await data('GET', 'pages/search?q=giraffe')).toHaveLength(1);
		});

		it('survives hostile input without errors or leaks', async () => {
			await create('Doc', { body: 'ordinary words here' });
			for (const q of [
				'"',
				'*',
				'NEAR(',
				'-',
				'"a" OR "b"',
				'title:x',
				'(((',
				'^',
				'a b c d e f g h i j k l m',
				'x'.repeat(5000),
				'__',
				"'; DROP TABLE pages; --",
				'NEAR(a b)',
				'AND',
				'{title}: foo',
			]) {
				const res = await call(
					'GET',
					`pages/search?q=${encodeURIComponent(q)}`,
				);
				expect(res.status).toBe(200);
				expect(Array.isArray(res.data.data)).toBe(true);
			}
			expect(await data('GET', 'pages/search?q=')).toEqual([]);
			expect(await data('GET', 'pages/search?q=a')).toEqual([]);
			expect(await data('GET', 'pages/search')).toEqual([]);
			expect(await data('GET', 'pages/search?q=ordinary')).toHaveLength(1);
			expect(await data('GET', 'pages/search?q=ord*')).toHaveLength(1);
			expect(
				(await ctx.db.select(`SELECT COUNT(*) AS n FROM pages`))[0].n,
			).toBe(1);
		});

		it('limits the hits', async () => {
			for (let i = 0; i < 4; i++)
				await create(`Note ${i}`, { body: 'common word' });
			expect(await data('GET', 'pages/search?q=common&limit=2')).toHaveLength(
				2,
			);
			expect(await data('GET', 'pages/search?q=common')).toHaveLength(4);
		});
	});

	describe('files', () => {
		it('attaches an uploaded file to a page and lists it, reusing the same key', async () => {
			const page = await create('Doc');
			const file = await data('POST', `pages/${page.id}/files`, {
				file_name: 'shot.png',
				file_path: 'abc-123/shot.png',
				mime_type: 'image/png',
				size_bytes: 12,
			});
			expect(file).toMatchObject({
				page_id: page.id,
				name: 'shot.png',
				mime_type: 'image/png',
				size: 12,
			});
			expect(
				(
					await data('POST', `pages/${page.id}/files`, {
						file_name: 'shot.png',
						file_path: 'abc-123/shot.png',
					})
				).id,
			).toBe(file.id);
			expect(
				(await data('GET', `pages/${page.id}/files`)).map((f: any) => f.id),
			).toEqual([file.id]);
			expect(
				(await data('GET', `pages/${page.id}`)).files.map((f: any) => f.id),
			).toEqual([file.id]);
			expect((await data('GET', `files/${file.id}`)).name).toBe('shot.png');
			const other = await create('Other');
			const taken = await call('POST', `pages/${other.id}/files`, {
				file_name: 'x',
				file_path: 'abc-123/shot.png',
			});
			expect([taken.status, taken.data.error]).toEqual([
				422,
				'invalid_file_path',
			]);
			const bad = await call('POST', `pages/${page.id}/files`, {
				file_name: 'x',
				file_path: '../etc/passwd',
			});
			expect(bad.data.error).toBe('invalid_file_path');
			expect(
				(await call('POST', `pages/${page.id}/files`, {})).data.error,
			).toBe('invalid_file');
		});

		it('binds an existing page file by id and refuses a task file', async () => {
			const page = await create('Doc');
			const task = await data('POST', 'tasks', { title: 'T' });
			const taskFile = await data('POST', `tasks/${task.id}/files`, {
				file_path: 'u-1/a.png',
				file_name: 'a.png',
			});
			expect(
				(await call('POST', `pages/${page.id}/files`, { file_id: taskFile.id }))
					.data.error,
			).toBe('file_bound_to_task');
			expect(
				(await call('POST', `pages/${page.id}/files`, { file_id: 9999 })).data
					.error,
			).toBe('file_not_found');
		});
	});

	describe('task from selection', () => {
		it('creates a task, replaces the first occurrence with its key and links back', async () => {
			const category = await data('POST', 'project_categories', {
				title: 'Team',
				code: 'TM',
			});
			const page = await create('Meeting', {
				body: '## Действия\n\nCall the vendor\nCall the vendor\n',
			});
			const res = await call('POST', `pages/${page.id}/task-from-selection`, {
				text: 'Call the vendor',
				category_id: category.id,
				version: 1,
			});
			expect(res.status).toBe(201);
			const { task, page: updated } = res.data.data;
			expect(task).toMatchObject({
				title: 'Call the vendor',
				key: `TM-${task.category_tasks_sequence_id}`,
			});
			expect(task.description).toContain(
				`Из страницы: [Meeting](tmgr://page/${page.id})`,
			);
			expect(updated.body).toBe(
				`## Действия\n\n[${task.key}](tmgr://task/${task.id})\nCall the vendor\n`,
			);
			expect(updated.version).toBe(2);
			expect(
				(await data('GET', `tasks/${task.id}/pages`)).map((p: any) => p.id),
			).toEqual([page.id]);
			expect(
				await ctx.db.select(
					`SELECT * FROM task_page_mentions WHERE task_id = ?`,
					[task.id],
				),
			).toHaveLength(1);
		});

		it('validates the request: version, text, category, stale version and missing selection', async () => {
			const category = await data('POST', 'project_categories', {
				title: 'Team',
				code: 'TM',
			});
			const page = await create('Meeting', { body: 'hello world' });
			const url = `pages/${page.id}/task-from-selection`;
			expect(
				(await call('POST', url, { text: 'hello', category_id: category.id }))
					.data.error,
			).toBe('version_required');
			expect(
				(await call('POST', url, { category_id: category.id, version: 1 })).data
					.error,
			).toBe('text_required');
			expect(
				(await call('POST', url, { text: 'hello', version: 1 })).data.error,
			).toBe('category_required');
			const missing = await call('POST', url, {
				text: 'nope',
				category_id: category.id,
				version: 1,
			});
			expect([missing.status, missing.data.error]).toEqual([
				422,
				'selection_not_found',
			]);
			await data('PATCH', `pages/${page.id}`, {
				version: 1,
				body: 'hello world!',
			});
			const stale = await call('POST', url, {
				text: 'hello',
				category_id: category.id,
				version: 1,
			});
			expect([stale.status, stale.data.data.version]).toEqual([409, 2]);
			expect((await data('GET', 'tasks')).length).toBe(0);
		});
	});

	describe('misc', () => {
		it('treats follow and unfollow as a no-op with 204', async () => {
			const page = await create('Doc');
			expect((await call('POST', `pages/${page.id}/follow`)).status).toBe(204);
			expect((await call('DELETE', `pages/${page.id}/follow`)).status).toBe(
				204,
			);
		});

		it('reports the pages feature as enabled for local workspaces', async () => {
			const toggles = await data('GET', 'workspaces/-1/feature-toggles');
			expect(toggles.pages).toMatchObject({ key: 'pages', enabled: true });
		});

		it('joins all context pages into the workspace context document', async () => {
			await create('Контекст', { type: 'context' });
			const { markdown } = await data('GET', 'workspaces/context');
			expect(markdown).toContain('# Контекст');
			expect(markdown).toContain('## Как мы работаем');
		});
	});

	describe('domain events', () => {
		let events: DomainEvent[];
		let off: () => void;
		beforeEach(() => {
			events = [];
			off = domainEvents.on((event) => {
				if (event.type.startsWith('page.')) events.push(event);
			});
		});
		afterEach(() => off());

		it('publishes created, updated, moved, deleted and restored with the realtime payload shape', async () => {
			const page = await create('Doc');
			await data('PATCH', `pages/${page.id}`, {
				version: 1,
				body: 'x',
				summary: 'edit',
			});
			await data('POST', `pages/${page.id}/move`, {
				parent_id: null,
				position: 0,
			});
			await data('DELETE', `pages/${page.id}`);
			await data('POST', `pages/${page.id}/restore`);
			expect(events.map((e) => e.type)).toEqual([
				'page.created',
				'page.updated',
				'page.moved',
				'page.deleted',
				'page.restored',
			]);
			expect(events[1]).toMatchObject({
				workspaceId: -1,
				page: {
					id: page.id,
					slug: 'doc',
					title: 'Doc',
					type: 'plain',
					parent_id: null,
					version: 2,
					summary: 'edit',
					linked_task_ids: [],
					updated_by: { kind: 'user' },
				},
			});
			expect((events[1] as any).actor).toBeUndefined();
		});

		it('tags persona and plugin writes with their actor and reports changed task mentions', async () => {
			const task = await data('POST', 'tasks', { title: 'T' });
			const page = await create('Doc', { body: 'x' });
			await data(
				'POST',
				`pages/${page.id}/append`,
				{ markdown: `see [t](tmgr://task/${task.id})` },
				persona,
			);
			await data(
				'POST',
				`pages/${page.id}/append`,
				{ markdown: 'more' },
				plugin,
			);
			const updated = events.filter((e) => e.type === 'page.updated');
			expect(updated.map((e: any) => e.actor)).toEqual([
				'persona:p-1',
				'plugin:tmgr.people',
			]);
			expect((updated[0] as any).page.linked_task_ids).toEqual([task.id]);
			expect((updated[0] as any).page.updated_by).toMatchObject({
				kind: 'persona',
				id: 'p-1',
				name: 'Analyst',
			});
			expect((updated[1] as any).page.linked_task_ids).toEqual([]);
		});

		it('tags the promises rebuild of a person page with the system actor so an open page refreshes', async () => {
			const person = await create('Иван', { type: 'person' });
			events.length = 0;
			await data('POST', 'tasks', {
				title: 'Report',
				description: `for [x](tmgr://page/${person.id})`,
			});
			expect(events.map((e: any) => [e.type, e.actor, e.page.id])).toEqual([
				['page.updated', 'system', person.id],
			]);
		});

		it('publishes nothing when a write changes nothing or fails', async () => {
			const page = await create('Doc', { body: 'same' });
			events.length = 0;
			await data('PATCH', `pages/${page.id}`, { version: 1, body: 'same' });
			await call('PATCH', `pages/${page.id}`, { version: 99, body: 'x' });
			expect(events).toEqual([]);
		});
	});
});
