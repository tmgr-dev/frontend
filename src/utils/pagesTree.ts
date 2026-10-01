import type { PageSummary, PageType } from '@/actions/tmgr/pages';

export interface PageNode extends PageSummary {
	children: PageNode[];
}

export interface PageCreateOption {
	type: PageType;
	label: string;
}

export const DEFAULT_PAGE_TITLE = 'Untitled';

export const PAGE_CREATE_OPTIONS: PageCreateOption[] = [
	{ type: 'plain', label: 'Plain page' },
	{ type: 'context', label: 'Context' },
	{ type: 'person', label: 'Person' },
	{ type: 'meeting', label: 'Meeting' },
];

const compareSiblings = (a: PageSummary, b: PageSummary): number => {
	if (a.pinned !== b.pinned) return a.pinned ? -1 : 1;
	if (a.position !== b.position) return a.position - b.position;
	return a.id - b.id;
};

export const buildPagesTree = (pages: PageSummary[]): PageNode[] => {
	const nodes = new Map<number, PageNode>();
	pages.forEach((page) => nodes.set(page.id, { ...page, children: [] }));
	const roots: PageNode[] = [];
	nodes.forEach((node) => {
		const parent = node.parent_id === null ? null : nodes.get(node.parent_id);
		if (parent && parent !== node) parent.children.push(node);
		else roots.push(node);
	});
	const sortLevel = (level: PageNode[]) => {
		level.sort(compareSiblings);
		level.forEach((node) => sortLevel(node.children));
	};
	sortLevel(roots);
	return roots;
};

export const childrenOf = (
	pages: PageSummary[],
	parentId: number | null,
): PageSummary[] =>
	pages.filter((page) => page.parent_id === parentId).sort(compareSiblings);

export const applyMove = (
	pages: PageSummary[],
	id: number,
	parentId: number | null,
	position: number,
): PageSummary[] => {
	const moved = pages.find((page) => page.id === id);
	if (!moved) return pages;
	const siblings = childrenOf(
		pages.filter((page) => page.id !== id),
		parentId,
	);
	const index = Math.max(0, Math.min(position, siblings.length));
	siblings.splice(index, 0, { ...moved, parent_id: parentId });
	const placed = new Map<number, PageSummary>();
	siblings.forEach((page, i) => placed.set(page.id, { ...page, position: i }));
	return pages.map((page) => placed.get(page.id) ?? page);
};

export const descendantIds = (
	pages: PageSummary[],
	id: number,
): Set<number> => {
	const result = new Set<number>([id]);
	let grew = true;
	while (grew) {
		grew = false;
		pages.forEach((page) => {
			if (
				page.parent_id !== null &&
				result.has(page.parent_id) &&
				!result.has(page.id)
			) {
				result.add(page.id);
				grew = true;
			}
		});
	}
	return result;
};

export const ancestorIds = (pages: PageSummary[], id: number): number[] => {
	const byId = new Map(pages.map((page) => [page.id, page]));
	const result: number[] = [];
	let current = byId.get(id);
	while (
		current &&
		current.parent_id !== null &&
		!result.includes(current.parent_id)
	) {
		result.push(current.parent_id);
		current = byId.get(current.parent_id);
	}
	return result;
};

const expandedKey = (workspaceId: number | string) =>
	`pages-expanded-${workspaceId}`;

export const loadExpanded = (workspaceId: number | string): Set<number> => {
	try {
		const raw = localStorage.getItem(expandedKey(workspaceId));
		const parsed = raw ? JSON.parse(raw) : [];
		return new Set(
			Array.isArray(parsed) ? parsed.filter((v) => typeof v === 'number') : [],
		);
	} catch {
		return new Set();
	}
};

export const saveExpanded = (
	workspaceId: number | string,
	expanded: Set<number>,
): void => {
	try {
		localStorage.setItem(
			expandedKey(workspaceId),
			JSON.stringify(Array.from(expanded)),
		);
	} catch {
		return;
	}
};

export const pagesAvailable = (
	workspace: { id?: number | string; is_local?: boolean } | null | undefined,
	toggleEnabled: boolean,
): boolean => !!workspace && toggleEnabled;

export const pageUrl = (workspaceCode: string, slug: string): string =>
	`/${workspaceCode}/pages/${encodeURIComponent(slug)}`;
