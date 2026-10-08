import type { Page, PageSummary } from '@/actions/tmgr/pages';
import {
	buildExportBundle,
	buildExportTree,
	exportSinglePage,
	type ExportProvider,
} from '../exportBundle';
import { buildImportPlan } from '../importPlan';
import { runImportWith } from '../importRun';
import { readVirtualFiles, readZip } from '../zip';
import { createFakeApi } from './fakeApi';

const summary = (
	id: number,
	title: string,
	parent: number | null,
	position = 0,
): PageSummary => ({
	id,
	title,
	slug: title.toLowerCase().replace(/\W+/g, '-'),
	type: 'plain',
	parent_id: parent,
	position,
	pinned: false,
	updated_at: '',
});

const tree: PageSummary[] = [
	summary(1, 'Handbook', null),
	summary(2, 'Setup: step 1', 1),
	summary(3, 'Setup: step 1', 1, 1),
	summary(4, 'Notes', null, 1),
];

const bodies: Record<number, string> = {
	1: 'See [setup](tmgr://page/2) and [slug](tmgr://page/acme/notes).\n\n![shot](tmgr://file/9)\n\n`[code](tmgr://page/2)`',
	2: 'Back to [handbook](tmgr://page/1#top). Task [t](tmgr://task/77).',
	3: 'second twin',
	4: 'Notes body',
};

const png = new Uint8Array([137, 80, 78, 71, 1, 2, 3]);

const provider: ExportProvider = {
	getPage: async (id) =>
		({
			...tree.find((p) => p.id === id)!,
			body: bodies[id],
			properties: {},
			version: 1,
			files:
				id === 1
					? [{ id: 9, name: 'shot.png', original_name: 'My shot.png' }]
					: [],
		} as unknown as Page),
	fetchFile: async (id) => {
		if (id === 9) return png;
		throw new Error('missing');
	},
};

describe('export to import round trip', () => {
	it('keeps titles, tree, links and files', async () => {
		const bundle = await buildExportBundle({
			roots: buildExportTree(tree, null),
			workspaceCode: 'acme',
			zipName: 'acme-pages.zip',
			provider,
		});
		expect(bundle.fileName).toBe('acme-pages.zip');
		expect(bundle.warnings).toEqual([]);

		const entries = readZip(bundle.bytes)
			.files.map((f) => f.path)
			.sort();
		expect(entries).toEqual([
			'Handbook.md',
			'Handbook/Setup- step 1 (2).md',
			'Handbook/Setup- step 1.md',
			'Notes.md',
			'assets/9-My shot.png',
		]);
		const exported = new TextDecoder().decode(
			readZip(bundle.bytes).files.find((f) => f.path === 'Handbook.md')!.bytes,
		);
		expect(exported).toContain('[setup](Handbook/Setup-%20step%201.md)');
		expect(exported).toContain('[slug](Notes.md)');
		expect(exported).toContain('![shot](assets/9-My%20shot.png)');
		expect(exported).toContain('`[code](tmgr://page/2)`');

		const read = readVirtualFiles([
			{ name: 'acme-pages.zip', bytes: bundle.bytes },
		]);
		const plan = buildImportPlan(read.files, { existingTitles: [] });
		expect(plan.warnings).toEqual([]);
		const api = createFakeApi();
		const result = await runImportWith(api, plan, {
			parentId: null,
			policy: 'rename',
			workspaceCode: 'acme',
		});
		expect(result.error).toBeNull();
		expect(result.warnings).toEqual([]);

		const created = result.created;
		const idOf = (title: string, nth = 0) =>
			created.filter((c) => c.title === title)[nth].id;
		expect(created.map((c) => c.title).sort()).toEqual([
			'Handbook',
			'Notes',
			'Setup: step 1',
			'Setup: step 1',
		]);
		const handbook = idOf('Handbook');
		expect(api.pages.get(idOf('Setup: step 1', 0))!.parent_id).toBe(handbook);
		expect(api.pages.get(idOf('Setup: step 1', 1))!.parent_id).toBe(handbook);
		expect(api.pages.get(handbook)!.parent_id).toBeNull();

		const upload = api.uploads[0];
		expect(upload.pageId).toBe(handbook);
		expect(upload.file.name).toBe('9-My shot.png');
		expect([...upload.file.bytes]).toEqual([...png]);
		const setup = [...api.pages.values()].find((p) =>
			p.body.startsWith('Back to'),
		)!;
		expect(api.pages.get(handbook)!.body).toBe(
			`See [setup](tmgr://page/${setup.id}) and [slug](tmgr://page/${idOf(
				'Notes',
			)}).\n\n![shot](tmgr://file/${
				upload.fileId
			})\n\n\`[code](tmgr://page/2)\``,
		);
		expect(setup.body).toBe(
			`Back to [handbook](tmgr://page/${handbook}). Task [t](tmgr://task/77).`,
		);
	});

	it('warns and keeps the link when an asset cannot be fetched', async () => {
		const bundle = await buildExportBundle({
			roots: buildExportTree(tree, 1),
			workspaceCode: 'acme',
			zipName: 'handbook.zip',
			provider: {
				...provider,
				fetchFile: async () => {
					throw new Error('nope');
				},
			},
		});
		expect(bundle.warnings).toHaveLength(1);
		const text = new TextDecoder().decode(
			readZip(bundle.bytes).files.find((f) => f.path === 'Handbook.md')!.bytes,
		);
		expect(text).toContain('![shot](tmgr://file/9)');
	});

	it('exports a single page with frontmatter and the body untouched', async () => {
		const page = (await provider.getPage(1)) as Page;
		const { fileName, text } = exportSinglePage(page, 'acme');
		expect(fileName).toBe('handbook.md');
		expect(
			text.startsWith(
				'---\ntitle: Handbook\ntype: plain\ntmgr:\n  workspace: acme\n  id: 1\n---\n\n',
			),
		).toBe(true);
		expect(text.endsWith(bodies[1])).toBe(true);
	});
});
