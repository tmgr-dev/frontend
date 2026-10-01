import type { PageSummary } from '@/actions/tmgr/pages';
import { headings, sections, SYSTEM_OWNER } from '@/local/pages/markdown';
import { buildPagesTree, type PageNode } from '@/utils/pagesTree';

const LAST_PAGE_KEY = 'desktop.quickAdd.page.';

export interface PageOption {
	id: number;
	title: string;
	depth: number;
}

export const listPageSections = (body: string): string[] => {
	const text = body || '';
	const managed = sections(text).filter((s) => s.owner === SYSTEM_OWNER);
	const seen = new Set<string>();
	const found: string[] = [];
	for (const heading of headings(text)) {
		if (heading.level !== 2 || !heading.text) continue;
		if (managed.some((s) => heading.start >= s.start && heading.start < s.end))
			continue;
		const key = heading.text.toLowerCase();
		if (seen.has(key)) continue;
		seen.add(key);
		found.push(heading.text);
	}
	return found;
};

export const composeAppendMarkdown = (
	text: string,
	fileId: number | null,
): string =>
	[fileId != null ? `![](tmgr://file/${fileId})` : '', (text || '').trim()]
		.filter(Boolean)
		.join('\n\n');

export const flattenPages = (pages: PageSummary[]): PageOption[] => {
	const options: PageOption[] = [];
	const walk = (nodes: PageNode[], depth: number) =>
		nodes.forEach((node) => {
			options.push({ id: node.id, title: node.title, depth });
			walk(node.children, depth + 1);
		});
	walk(buildPagesTree(pages), 0);
	return options;
};

export const filterPageOptions = (
	options: PageOption[],
	query: string,
	keepId: number | null = null,
): PageOption[] => {
	const needle = query.trim().toLowerCase();
	if (!needle) return options;
	return options.filter(
		(option) =>
			option.id === keepId || option.title.toLowerCase().includes(needle),
	);
};

export const pickRememberedPage = (
	options: PageOption[],
	remembered: number | null,
): number | null =>
	remembered !== null && options.some((option) => option.id === remembered)
		? remembered
		: null;

export const readLastPage = (workspaceId: number): number | null => {
	try {
		return Number(localStorage.getItem(LAST_PAGE_KEY + workspaceId)) || null;
	} catch {
		return null;
	}
};

export const writeLastPage = (workspaceId: number, pageId: number): void => {
	try {
		localStorage.setItem(LAST_PAGE_KEY + workspaceId, String(pageId));
	} catch {
		/* storage unavailable: the page is not remembered */
	}
};

export interface PageTarget {
	id: number;
	code: string;
}

interface AppendDeps {
	uploadPageFile: (
		pageId: number,
		file: File,
		workspaceId?: number,
	) => Promise<{ id: number }>;
	appendToPage: (
		pageId: number,
		payload: { markdown: string; heading?: string; workspace_id?: number },
	) => Promise<{ id: number; slug: string; title: string; version: number }>;
	relayPageAppended: (payload: {
		workspace_code: string;
		page: { id: number; slug: string; title: string; version: number };
	}) => Promise<void>;
}

/** Everything goes to the workspace the page list was loaded from, so a switch mid-way fails closed instead of writing elsewhere. */
export const appendToLoadedPage = async (
	workspace: PageTarget,
	pageId: number,
	input: { text: string; section: string; screenshot: File | null },
	deps: AppendDeps,
) => {
	const fileId = input.screenshot
		? (await deps.uploadPageFile(pageId, input.screenshot, workspace.id)).id
		: null;
	const updated = await deps.appendToPage(pageId, {
		markdown: composeAppendMarkdown(input.text, fileId),
		heading: input.section || undefined,
		workspace_id: workspace.id,
	});
	void deps
		.relayPageAppended({
			workspace_code: workspace.code,
			page: {
				id: updated.id,
				slug: updated.slug,
				title: updated.title,
				version: updated.version,
			},
		})
		.catch((e) => console.error('quick add: page-appended relay failed', e));
	return updated;
};
