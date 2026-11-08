import type { PageSearchHit } from '@/actions/tmgr/pages';
import { t } from '@/i18n';
import { pageUrl } from '@/utils/pagesTree';

export const MIN_SEARCH_LENGTH = 2;
export const SEARCH_DEBOUNCE_MS = 300;

export const normalizeQuery = (query: string): string => query.trim();

export const canSearch = (query: string): boolean =>
	normalizeQuery(query).length >= MIN_SEARCH_LENGTH;

export const pageHitUrl = (
	workspaceCode: string,
	hit: Pick<PageSearchHit, 'slug'>,
): string => pageUrl(workspaceCode, hit.slug);

const PAGE_TYPES = ['plain', 'context', 'person', 'meeting'];

export const pageTypeLabel = (type: string): string =>
	PAGE_TYPES.includes(type) ? t(`pages.helpers.pageType.${type}`) : type;

export const cleanSnippet = (snippet: string): string =>
	snippet
		.replace(/<!--[\s\S]*?-->/g, '')
		.replace(/!\[[^\]]*\]\([^)]*\)/g, '')
		.replace(/\[([^\]]*)\]\([^)]*\)/g, '$1')
		.replace(/#{1,6}\s+/g, '')
		.replace(/\s+/g, ' ')
		.trim();
