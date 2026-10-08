import type { Page, PageSummary } from '@/actions/tmgr/pages';
import { parseTmgrUrl } from '@/utils/pages/tmgrLinks';
import { serializeFrontmatter } from './frontmatter';
import { collectLinks, rewriteForExport, splitTargetFragment } from './links';
import { sanitizeFileName, uniqueName } from './paths';
import type { IoWarning, VirtualFile } from './types';
import { writeZip } from './zip';

export interface ExportProvider {
	getPage(id: number): Promise<Page>;
	fetchFile(id: number): Promise<Uint8Array>;
}

export interface ExportTreeNode {
	summary: PageSummary;
	children: ExportTreeNode[];
}

export interface ExportBundle {
	fileName: string;
	bytes: Uint8Array;
	warnings: IoWarning[];
}

const FETCH_CONCURRENCY = 4;
const encoder = new TextEncoder();

const mapLimit = async <T, R>(
	items: T[],
	limit: number,
	fn: (item: T) => Promise<R>,
): Promise<R[]> => {
	const results: R[] = new Array(items.length);
	let next = 0;
	const worker = async () => {
		while (next < items.length) {
			const index = next;
			next += 1;
			results[index] = await fn(items[index]);
		}
	};
	await Promise.all(
		Array.from({ length: Math.min(limit, items.length) }, worker),
	);
	return results;
};

const byPosition = (a: PageSummary, b: PageSummary): number =>
	a.position - b.position || a.title.localeCompare(b.title);

export const buildExportTree = (
	summaries: PageSummary[],
	rootId: number | null,
): ExportTreeNode[] => {
	const live = summaries.filter((page) => !page.deleted_at);
	const childrenOf = (parentId: number | null): ExportTreeNode[] =>
		live
			.filter((page) => page.parent_id === parentId)
			.sort(byPosition)
			.map((summary) => ({ summary, children: childrenOf(summary.id) }));
	if (rootId === null) return childrenOf(null);
	const root = live.find((page) => page.id === rootId);
	return root ? [{ summary: root, children: childrenOf(root.id) }] : [];
};

const document = (page: Page, workspaceCode: string, body: string): string =>
	serializeFrontmatter({
		title: page.title,
		type: page.type,
		properties: page.properties,
		tmgr: { workspace: workspaceCode, id: page.id },
	}) + body;

export const exportSinglePage = (
	page: Page,
	workspaceCode: string,
): { fileName: string; text: string } => ({
	fileName: `${sanitizeFileName(page.slug || page.title)}.md`,
	text: document(page, workspaceCode, page.body ?? ''),
});

const flatten = (
	nodes: ExportTreeNode[],
	dir: string,
	paths: Map<number, string>,
): void => {
	const taken = new Set<string>();
	for (const node of nodes) {
		const name = uniqueName(sanitizeFileName(node.summary.title), taken);
		paths.set(node.summary.id, `${dir}${name}.md`);
		flatten(node.children, `${dir}${name}/`, paths);
	}
};

const fileRefs = (body: string): number[] => {
	const ids: number[] = [];
	for (const token of collectLinks(body)) {
		if (token.kind !== 'link' && token.kind !== 'image') continue;
		const parsed = parseTmgrUrl(splitTargetFragment(token.target).base);
		if (parsed?.form === 'storage' && parsed.kind === 'file') {
			ids.push(Number(parsed.id));
		}
	}
	return ids;
};

export const buildExportBundle = async (options: {
	roots: ExportTreeNode[];
	workspaceCode: string;
	zipName: string;
	provider: ExportProvider;
}): Promise<ExportBundle> => {
	const { roots, workspaceCode, provider } = options;
	const warnings: IoWarning[] = [];
	const pagePaths = new Map<number, string>();
	flatten(roots, '', pagePaths);

	const ids = [...pagePaths.keys()];
	const pages = await mapLimit(ids, FETCH_CONCURRENCY, (id) =>
		provider.getPage(id),
	);

	const pageSlugPaths = new Map<string, string>();
	const fileNames = new Map<number, string>();
	const wanted = new Set<number>();
	for (const page of pages) {
		pageSlugPaths.set(page.slug, pagePaths.get(page.id) as string);
		for (const file of page.files ?? []) {
			fileNames.set(file.id, file.original_name || file.name);
		}
		for (const id of fileRefs(page.body ?? '')) wanted.add(id);
	}

	const filePaths = new Map<number, string>();
	const assets: VirtualFile[] = [];
	await mapLimit([...wanted], FETCH_CONCURRENCY, async (id) => {
		try {
			const bytes = await provider.fetchFile(id);
			const path = `assets/${id}-${sanitizeFileName(
				fileNames.get(id) ?? 'file',
			)}`;
			filePaths.set(id, path);
			assets.push({ path, bytes });
		} catch {
			warnings.push({
				path: null,
				message: `Attachment ${id} could not be downloaded; its link was left as is`,
			});
		}
	});

	const files: VirtualFile[] = pages.map((page) => {
		const path = pagePaths.get(page.id) as string;
		const body = rewriteForExport(page.body ?? '', {
			fromPath: path,
			workspaceCode,
			pagePaths,
			pageSlugPaths,
			filePaths,
		});
		return {
			path,
			bytes: encoder.encode(document(page, workspaceCode, body)),
		};
	});
	assets.sort((a, b) => a.path.localeCompare(b.path));

	return {
		fileName: options.zipName,
		bytes: writeZip([...files, ...assets]),
		warnings,
	};
};
