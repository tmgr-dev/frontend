import type { PageSummary } from '@/actions/tmgr/pages';
import { buildPagesTree, type PageNode } from '@/utils/pagesTree';

const HEADING = /^##\s+(.+?)\s*#*\s*$/;
const FENCE = /^ {0,3}(`{3,}|~{3,})/;
const LAST_PAGE_KEY = 'desktop.quickAdd.page.';

export interface PageOption {
	id: number;
	title: string;
	depth: number;
}

export const listPageSections = (body: string): string[] => {
	const seen = new Set<string>();
	const sections: string[] = [];
	let fence: string | null = null;
	for (const line of (body || '').split(/\r?\n/)) {
		const fenceMatch = FENCE.exec(line);
		if (fenceMatch) {
			const mark = fenceMatch[1][0];
			if (fence === null) fence = mark;
			else if (fence === mark) fence = null;
			continue;
		}
		if (fence !== null) continue;
		const match = HEADING.exec(line);
		const text = match?.[1].trim();
		if (!text) continue;
		const key = text.toLowerCase();
		if (seen.has(key)) continue;
		seen.add(key);
		sections.push(text);
	}
	return sections;
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
