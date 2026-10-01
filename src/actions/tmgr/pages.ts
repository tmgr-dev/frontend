import $axios from '@/plugins/axios';
import store from '@/store';
import { requestCache } from '@/utils/requestCache';

export type PageType = 'plain' | 'context' | 'person' | 'meeting';

export interface PageAuthor {
	kind: 'user' | 'persona' | 'plugin';
	id: string | number | null;
	name: string | null;
	owner?: { id: string | number | null; name: string | null };
	avatar?: string | null;
}

export interface PageSummary {
	id: number;
	title: string;
	slug: string;
	type: PageType;
	parent_id: number | null;
	position: number;
	pinned: boolean;
	updated_at: string;
	deleted_at?: string | null;
}

export interface PageSection {
	id: string;
	owner: string;
	heading: string | null;
}

export interface Page extends PageSummary {
	workspace_id: number;
	body: string;
	properties: Record<string, any>;
	version: number;
	author: PageAuthor;
	updated_by: PageAuthor;
	created_at: string;
	backlinks: PageSummary[];
	sections: PageSection[];
}

export interface PageVersion {
	version: number;
	title: string;
	author: PageAuthor;
	summary: string | null;
	created_at: string;
}

export interface PageVersionSnapshot extends PageVersion {
	body: string;
	properties: Record<string, any>;
}

export interface PageSearchHit {
	id: number;
	slug: string;
	title: string;
	type: PageType;
	snippet: string;
	updated_at: string;
}

export interface CreatePagePayload {
	title: string;
	type?: PageType;
	parent_id?: number | null;
	body?: string;
	properties?: Record<string, any>;
}

export interface UpdatePagePayload {
	version: number;
	title?: string;
	body?: string;
	properties?: Record<string, any>;
	summary?: string;
}

export interface AppendPagePayload {
	markdown: string;
	heading?: string;
	create_heading?: boolean;
	summary?: string;
}

export class PageConflictError extends Error {
	current: Page;

	constructor(current: Page) {
		super('page_conflict');
		this.name = 'PageConflictError';
		this.current = current;
	}
}

const TREE_KEY = 'pages-tree';
const pageKey = (idOrSlug: number | string) => `pages-page-${idOrSlug}`;

export const invalidatePages = () => requestCache.invalidate(/^pages-/);

const unwrap = async <T>(request: Promise<any>): Promise<T> => {
	const {
		data: { data },
	} = await request;
	return data;
};

const rethrowConflict = (error: any): never => {
	const response = error?.response;
	if (response?.status === 409 && response.data?.error === 'page_conflict') {
		throw new PageConflictError(response.data.data);
	}
	throw error;
};

const mutate = async <T>(request: Promise<any>): Promise<T> => {
	try {
		const result = await unwrap<T>(request);
		invalidatePages();
		store.commit('pagesEvent', { type: 'page.local', page: null });
		return result;
	} catch (error) {
		invalidatePages();
		return rethrowConflict(error);
	}
};

export const getPages = async (
	params: { parent_id?: number | null; type?: PageType } = {},
): Promise<PageSummary[]> => unwrap($axios.get('pages', { params }));

export const getPagesTree = async (useCache = true): Promise<PageSummary[]> =>
	requestCache.getOrFetch<PageSummary[]>(
		TREE_KEY,
		() => unwrap($axios.get('pages/tree')),
		{ ttl: 60000, cache: useCache },
	);

export const getPagesTrash = async (): Promise<PageSummary[]> =>
	unwrap($axios.get('pages/trash'));

export const getPage = async (
	idOrSlug: number | string,
	useCache = false,
): Promise<Page> =>
	requestCache.getOrFetch<Page>(
		pageKey(idOrSlug),
		() => unwrap($axios.get(`pages/${encodeURIComponent(idOrSlug)}`)),
		{ ttl: 30000, cache: useCache },
	);

export const createPage = async (payload: CreatePagePayload): Promise<Page> =>
	mutate($axios.post('pages', payload));

export const updatePage = async (
	id: number,
	payload: UpdatePagePayload,
): Promise<Page> => mutate($axios.patch(`pages/${id}`, payload));

export const appendToPage = async (
	id: number,
	payload: AppendPagePayload,
): Promise<Page> => mutate($axios.post(`pages/${id}/append`, payload));

export const setPageSection = async (
	id: number,
	sectionId: string,
	markdown: string,
	summary?: string,
): Promise<Page> =>
	mutate(
		$axios.put(`pages/${id}/sections/${encodeURIComponent(sectionId)}`, {
			markdown,
			summary,
		}),
	);

export const movePage = async (
	id: number,
	parentId: number | null,
	position: number,
): Promise<Page> =>
	mutate($axios.post(`pages/${id}/move`, { parent_id: parentId, position }));

export const pinPage = async (id: number): Promise<Page> =>
	mutate($axios.post(`pages/${id}/pin`));

export const unpinPage = async (id: number): Promise<Page> =>
	mutate($axios.post(`pages/${id}/unpin`));

export const deletePage = async (id: number): Promise<{ deleted: number }> =>
	mutate($axios.delete(`pages/${id}`));

export const restorePage = async (id: number): Promise<Page> =>
	mutate($axios.post(`pages/${id}/restore`));

export const getPageVersions = async (
	idOrSlug: number | string,
): Promise<PageVersion[]> =>
	unwrap($axios.get(`pages/${encodeURIComponent(idOrSlug)}/versions`));

export const getPageVersion = async (
	idOrSlug: number | string,
	version: number,
): Promise<PageVersionSnapshot> =>
	unwrap(
		$axios.get(`pages/${encodeURIComponent(idOrSlug)}/versions/${version}`),
	);

export const restorePageVersion = async (
	id: number,
	version: number,
): Promise<Page> =>
	mutate($axios.post(`pages/${id}/versions/${version}/restore`));

export const getPageBacklinks = async (
	idOrSlug: number | string,
): Promise<PageSummary[]> =>
	unwrap($axios.get(`pages/${encodeURIComponent(idOrSlug)}/backlinks`));

export const searchPages = async (
	q: string,
	params: { type?: PageType; limit?: number } = {},
): Promise<PageSearchHit[]> =>
	unwrap($axios.get('pages/search', { params: { q, ...params } }));

export const getTaskPages = async (taskId: number): Promise<PageSummary[]> =>
	unwrap($axios.get(`tasks/${taskId}/pages`));
