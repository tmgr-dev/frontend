import type { Page, PageSummary } from '@/actions/tmgr/pages';
import {
	buildExportBundle,
	buildExportTree,
	type ExportProvider,
} from '../exportBundle';
import { sanitizeFileName } from '../paths';
import { WorkspaceChangedError } from '../types';
import { readZip } from '../zip';

const summary = (
	id: number,
	title: string,
	parent: number | null = null,
): PageSummary => ({
	id,
	title,
	slug: `p${id}`,
	type: 'plain',
	parent_id: parent,
	position: id,
	pinned: false,
	updated_at: '',
});

const providerFor = (
	bodies: Record<number, string>,
	tree: PageSummary[],
	overrides: Partial<ExportProvider> = {},
): ExportProvider => ({
	getPage: async (id) =>
		({
			...tree.find((p) => p.id === id)!,
			body: bodies[id],
			properties: {},
			version: 1,
			files: [{ id: 9, name: 'a.png', original_name: 'a.png' }],
		} as unknown as Page),
	fetchFile: async () => new Uint8Array([1]),
	...overrides,
});

const bundle = (tree: PageSummary[], provider: ExportProvider) =>
	buildExportBundle({
		roots: buildExportTree(tree, null),
		workspaceCode: 'acme',
		zipName: 'x.zip',
		provider,
	});

const decode = (bytes: Uint8Array) => new TextDecoder().decode(bytes);

describe('buildExportBundle', () => {
	it('collects and rewrites reference definitions and nested images', async () => {
		const tree = [summary(1, 'A'), summary(2, 'B')];
		const result = await bundle(
			tree,
			providerFor(
				{
					1: '![i][f]\n\n[f]: tmgr://file/9\n[p]: tmgr://page/2',
					2: '[![x](tmgr://file/9)](tmgr://page/1)',
				},
				tree,
			),
		);
		const files = readZip(result.bytes).files;
		expect(files.map((f) => f.path).sort()).toEqual([
			'A.md',
			'B.md',
			'assets/9-a.png',
		]);
		const a = decode(files.find((f) => f.path === 'A.md')!.bytes);
		expect(a).toContain('[f]: assets/9-a.png\n[p]: B.md');
		const b = decode(files.find((f) => f.path === 'B.md')!.bytes);
		expect(b).toContain('[![x](assets/9-a.png)](A.md)');
	});

	it('skips a page that fails to load and keeps exporting', async () => {
		const tree = [summary(1, 'A'), summary(2, 'Gone'), summary(3, 'C')];
		const provider = providerFor(
			{ 1: '[g](tmgr://page/2)', 2: '', 3: 'c' },
			tree,
			{
				getPage: async (id) => {
					if (id === 2) throw { response: { status: 404 } };
					return providerFor({ 1: '[g](tmgr://page/2)', 3: 'c' }, tree).getPage(
						id,
					);
				},
			},
		);
		const result = await bundle(tree, provider);
		expect(
			readZip(result.bytes)
				.files.map((f) => f.path)
				.sort(),
		).toEqual(['A.md', 'C.md']);
		expect(result.warnings).toHaveLength(1);
		expect(result.warnings[0].path).toBe('Gone.md');
		const a = decode(
			readZip(result.bytes).files.find((f) => f.path === 'A.md')!.bytes,
		);
		expect(a).toContain('[g](tmgr://page/2)');
	});

	it('aborts instead of skipping when the workspace changed', async () => {
		const tree = [summary(1, 'A')];
		const provider = providerFor({ 1: 'a' }, tree, {
			getPage: async () => {
				throw new WorkspaceChangedError('export');
			},
		});
		await expect(bundle(tree, provider)).rejects.toThrow(
			'The workspace changed during export',
		);
	});

	it('keeps a root page named assets out of the assets folder', async () => {
		const tree = [summary(1, 'Assets'), summary(2, 'Child', 1)];
		const result = await bundle(
			tree,
			providerFor({ 1: '![i](tmgr://file/9)', 2: 'c' }, tree),
		);
		expect(
			readZip(result.bytes)
				.files.map((f) => f.path)
				.sort(),
		).toEqual(['Assets (2).md', 'Assets (2)/Child.md', 'assets/9-a.png']);
	});
});

describe('sanitizeFileName', () => {
	it('normalizes to NFC', () => {
		expect(sanitizeFileName('Cafe\u0301')).toBe('Caf\u00e9');
	});

	it.each([
		['CON', 'CON-'],
		['nul', 'nul-'],
		['Com1', 'Com1-'],
		['lpt9', 'lpt9-'],
		['aux.txt', 'aux-.txt'],
		['console', 'console'],
		['COM10', 'COM10'],
	])('avoids the Windows reserved name %s', (input, expected) => {
		expect(sanitizeFileName(input)).toBe(expected);
	});
});
