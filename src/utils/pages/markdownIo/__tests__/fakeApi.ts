import type { Page } from '@/actions/tmgr/pages';
import type { ImportApi, ImportUpload } from '../importRun';

export interface FakeApi extends ImportApi {
	pages: Map<number, Page>;
	uploads: { pageId: number; fileId: number; file: ImportUpload }[];
	updates: { id: number; version: number; body?: string }[];
	failCreate: (title: string) => boolean;
	failUpload: (name: string) => boolean;
	conflictOnce: Set<number>;
	rejectProperties: boolean;
}

const slugOf = (title: string): string =>
	title
		.toLowerCase()
		.replace(/[^a-z0-9]+/g, '-')
		.replace(/^-|-$/g, '') || 'page';

export const seedPage = (
	api: FakeApi,
	title: string,
	parentId: number | null = null,
): Page => {
	const id = api.pages.size + 1;
	const page = {
		id,
		title,
		slug: `${slugOf(title)}-${id}`,
		type: 'plain',
		parent_id: parentId,
		body: '',
		properties: {},
		version: 1,
		files: [],
	} as unknown as Page;
	api.pages.set(id, page);
	return page;
};

export const createFakeApi = (): FakeApi => {
	const api: FakeApi = {
		pages: new Map(),
		uploads: [],
		updates: [],
		failCreate: () => false,
		failUpload: () => false,
		conflictOnce: new Set(),
		rejectProperties: false,
		async createPage(payload) {
			if (api.failCreate(payload.title)) {
				throw { response: { status: 500, data: { message: 'boom' } } };
			}
			if (api.rejectProperties && payload.properties) {
				throw { response: { status: 422, data: { message: 'invalid' } } };
			}
			const page = seedPage(api, payload.title, payload.parent_id ?? null);
			Object.assign(page, {
				type: payload.type ?? 'plain',
				body: payload.body ?? '',
				properties: payload.properties ?? {},
			});
			return { ...page };
		},
		async getPage(id) {
			return { ...(api.pages.get(id) as Page) };
		},
		async updatePage(id, payload) {
			const page = api.pages.get(id) as Page;
			if (api.conflictOnce.delete(id)) {
				page.version += 1;
				throw Object.assign(new Error('page_conflict'), {
					name: 'PageConflictError',
					current: { ...page },
				});
			}
			if (payload.version !== page.version) {
				throw Object.assign(new Error('page_conflict'), {
					name: 'PageConflictError',
					current: { ...page },
				});
			}
			api.updates.push({ id, version: payload.version, body: payload.body });
			if (payload.body !== undefined) page.body = payload.body;
			if (payload.properties) page.properties = payload.properties;
			page.version += 1;
			return { ...page };
		},
		async uploadFile(pageId, file) {
			if (api.failUpload(file.name)) throw new Error('upload failed');
			const fileId = 100 + api.uploads.length + 1;
			api.uploads.push({ pageId, fileId, file });
			return { id: fileId };
		},
		async listChildTitles(parentId) {
			return [...api.pages.values()]
				.filter((page) => page.parent_id === parentId)
				.map((page) => page.title);
		},
	};
	return api;
};

export const text = (value: string): Uint8Array =>
	new TextEncoder().encode(value);

export const file = (path: string, content: string | Uint8Array) => ({
	path,
	bytes: typeof content === 'string' ? text(content) : content,
});
