import { invalidatePages } from '@/actions/tmgr/pages';

export const QUICK_ADD_PAGE_APPENDED = 'quick-add://page-appended';

export interface PageAppendedPayload {
	workspace_code: string;
	page: { id: number; slug: string; title: string; version: number };
}

interface HostStore {
	getters: { currentWorkspace?: { code?: string } | null };
	commit: (mutation: string, payload?: unknown) => void;
}

export const handlePageAppended = (
	store: HostStore,
	payload: PageAppendedPayload | null | undefined,
): void => {
	if (!payload?.page || typeof payload.page.id !== 'number') return;
	invalidatePages();
	if (store.getters.currentWorkspace?.code !== payload.workspace_code) return;
	store.commit('pagesEvent', { type: 'page.updated', page: payload.page });
};

export const installQuickAddPageHost = async (store: HostStore) => {
	const { listen } = await import('@tauri-apps/api/event');
	await listen<PageAppendedPayload>(QUICK_ADD_PAGE_APPENDED, ({ payload }) =>
		handlePageAppended(store, payload),
	);
};

export const relayPageAppended = async (
	payload: PageAppendedPayload,
): Promise<void> => {
	const { emitTo } = await import('@tauri-apps/api/event');
	await emitTo('main', QUICK_ADD_PAGE_APPENDED, payload);
};
