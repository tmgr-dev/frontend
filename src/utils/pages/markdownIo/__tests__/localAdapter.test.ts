import { memoryDb, nodeSqliteAvailable } from '@/local/__tests__/nodeDb';
import { createLocalApi } from '@/local/api';
import { dispatchLocal } from '@/local/dispatch';
import { migrate } from '@/local/schema';
import type { LocalContext } from '@/local/types';
import { buildImportPlan } from '../importPlan';
import { runImportWith, type ImportApi } from '../importRun';
import { file, text } from './fakeApi';

const describeSqlite = nodeSqliteAvailable ? describe : describe.skip;

describeSqlite('import through the local pages API on SQLite', () => {
	let ctx: LocalContext;
	let clock: Date;
	let keySeq: number;
	const api = createLocalApi();

	const call = async (method: string, url: string, body?: unknown) => {
		clock = new Date(clock.getTime() + 1000);
		const res = await dispatchLocal(api, ctx, method, url, body);
		if (!res) throw new Error(`no route ${method} ${url}`);
		if (res.status >= 400) {
			throw Object.assign(new Error(res.data?.error ?? 'failed'), {
				response: res,
				...(res.status === 409 && res.data?.error === 'page_conflict'
					? { name: 'PageConflictError', current: res.data.data }
					: {}),
			});
		}
		return res.data?.data;
	};

	const localImportApi = (): ImportApi => ({
		createPage: (payload) => call('POST', 'pages', payload),
		getPage: (id) => call('GET', `pages/${id}`),
		updatePage: (id, payload) => call('PATCH', `pages/${id}`, payload),
		uploadFile: (pageId, { name, mime, bytes }) => {
			keySeq += 1;
			return call('POST', `pages/${pageId}/files`, {
				file_name: name,
				file_path: `import-${keySeq}/${name}`,
				mime_type: mime,
				size_bytes: bytes.length,
			});
		},
		listChildTitles: async (parentId) =>
			(await call('GET', 'pages/tree'))
				.filter((p: any) => p.parent_id === parentId)
				.map((p: any) => p.title),
	});

	beforeEach(async () => {
		clock = new Date('2026-10-01T10:00:00.000Z');
		keySeq = 0;
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
			user: { id: 7, name: 'Test User', email: 'me@example.com' },
			now: () => clock,
			files: {
				url: (key) => `tmgrfile://localhost/${key}`,
				read: async () => new Blob(['x']),
				remove: async () => {},
			},
		};
		await migrate(ctx.db, clock.toISOString());
	});

	it('creates the tree, rewrites links and attaches files in the local database', async () => {
		const plan = buildImportPlan(
			[
				file(
					'Guide.md',
					'# Guide\n\nSee [one](Guide/One.md) and ![pic](pic.png)',
				),
				file('Guide/One.md', 'Back to [guide](../Guide.md). [[Missing]]'),
				file('pic.png', new Uint8Array([1, 2, 3])),
				file(
					'Who.md',
					'---\ntype: person\nproperties:\n  company: Example Co\n  user_id: 5\ntmgr:\n  workspace: other\n  id: 3\n---\nPerson',
				),
				file(
					'Sync.md',
					`---\ntype: meeting\nproperties:\n  date: '2026-10-08'\n  participants: [tmgr://page/3]\n  related_tasks: [1]\ntmgr:\n  workspace: other\n  id: 4\n---\nNotes ${
						text('x').length
					}`,
				),
			],
			{ existingTitles: [] },
		);
		const result = await runImportWith(localImportApi(), plan, {
			parentId: null,
			policy: 'rename',
			workspaceCode: 'local-personal',
		});
		expect(result.error).toBeNull();

		const tree = await call('GET', 'pages/tree');
		const byTitle = Object.fromEntries(tree.map((p: any) => [p.title, p]));
		expect(Object.keys(byTitle).sort()).toEqual([
			'Guide',
			'One',
			'Sync',
			'Who',
		]);
		expect(byTitle.One.parent_id).toBe(byTitle.Guide.id);
		expect(byTitle.Guide.parent_id).toBeNull();

		const guide = await call('GET', `pages/${byTitle.Guide.id}`);
		expect(guide.files).toHaveLength(1);
		expect(guide.body).toBe(
			`See [one](tmgr://page/${byTitle.One.id}) and ![pic](tmgr://file/${guide.files[0].id})`,
		);
		const one = await call('GET', `pages/${byTitle.One.id}`);
		expect(one.body).toBe(
			`Back to [guide](tmgr://page/${byTitle.Guide.id}). [[Missing]]`,
		);
		expect(one.backlinks.map((p: any) => p.id)).toEqual([byTitle.Guide.id]);

		const who = await call('GET', `pages/${byTitle.Who.id}`);
		expect(who).toMatchObject({ type: 'person' });
		expect(who.properties.company).toBe('Example Co');
		expect(who.properties.user_id).toBeNull();
		const sync = await call('GET', `pages/${byTitle.Sync.id}`);
		expect(sync.properties).toMatchObject({
			date: '2026-10-08',
			participants: [`tmgr://page/${byTitle.Who.id}`],
			related_tasks: [],
		});
	});

	it('applies the rename policy against existing local pages', async () => {
		await call('POST', 'pages', { title: 'Guide' });
		const plan = buildImportPlan([file('Guide.md', 'x')], {
			existingTitles: ['Guide'],
		});
		const result = await runImportWith(localImportApi(), plan, {
			parentId: null,
			policy: 'rename',
			workspaceCode: 'local-personal',
		});
		expect(result.created[0].title).toBe('Guide (2)');
	});
});
