import type { PageSearchHit } from '@/actions/tmgr/pages';
import { pageUrl } from '@/utils/pagesTree';

export type SearchTab = 'tasks' | 'pages';

export const SEARCH_TABS: { key: SearchTab; label: string }[] = [
	{ key: 'tasks', label: 'Задачи' },
	{ key: 'pages', label: 'Страницы' },
];

export const MIN_SEARCH_LENGTH = 2;
export const SEARCH_DEBOUNCE_MS = 300;

export const availableSearchTabs = (pagesEnabled: boolean) =>
	SEARCH_TABS.filter((tab) => tab.key !== 'pages' || pagesEnabled);

export const resolveSearchTab = (
	tab: SearchTab,
	pagesEnabled: boolean,
): SearchTab => (tab === 'pages' && !pagesEnabled ? 'tasks' : tab);

export const normalizeQuery = (query: string): string => query.trim();

export const canSearch = (query: string): boolean =>
	normalizeQuery(query).length >= MIN_SEARCH_LENGTH;

export const pageHitUrl = (
	workspaceCode: string,
	hit: Pick<PageSearchHit, 'slug'>,
): string => pageUrl(workspaceCode, hit.slug);

export const PAGE_TYPE_LABELS: Record<string, string> = {
	plain: 'Страница',
	context: 'Контекст',
	person: 'Человек',
	meeting: 'Встреча',
};

export const pageTypeLabel = (type: string): string =>
	PAGE_TYPE_LABELS[type] ?? type;

export const cleanSnippet = (snippet: string): string =>
	snippet
		.replace(/<!--[\s\S]*?-->/g, '')
		.replace(/!\[[^\]]*\]\([^)]*\)/g, '')
		.replace(/\[([^\]]*)\]\([^)]*\)/g, '$1')
		.replace(/#{1,6}\s+/g, '')
		.replace(/\s+/g, ' ')
		.trim();
