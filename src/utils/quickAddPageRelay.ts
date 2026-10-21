import { invalidatePages } from '@/actions/tmgr/pages';
import { tourBus } from '@/tours/events';
import { requestCache } from '@/utils/requestCache';
import { ROUTINE_SNAPSHOTS } from '@/utils/routineSnapshotInvalidation';

export const QUICK_ADD_PAGE_APPENDED = 'quick-add://page-appended';
export const QUICK_ADD_CREATED = 'quick-add://created';

export type QuickAddKind = 'task' | 'routine';

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

export const handleQuickAddCreated = (
	payload: { kind?: string } | null | undefined,
): void => {
	const kind = payload?.kind;
	if (kind !== 'task' && kind !== 'routine') return;
	if (kind === 'routine') requestCache.invalidate(ROUTINE_SNAPSHOTS);
	tourBus.emit('task.created', { source: 'quick_add', kind });
};

export const installQuickAddPageHost = async (store: HostStore) => {
	const { listen } = await import('@tauri-apps/api/event');
	await Promise.all([
		listen<{ kind?: string }>(QUICK_ADD_CREATED, ({ payload }) =>
			handleQuickAddCreated(payload),
		),
		listen<PageAppendedPayload>(QUICK_ADD_PAGE_APPENDED, ({ payload }) =>
			handlePageAppended(store, payload),
		),
	]);
};

export const relayPageAppended = async (
	payload: PageAppendedPayload,
): Promise<void> => {
	const { emitTo } = await import('@tauri-apps/api/event');
	await emitTo('main', QUICK_ADD_PAGE_APPENDED, payload);
};

export const relayCreated = async (kind: QuickAddKind): Promise<void> => {
	try {
		const { emitTo } = await import('@tauri-apps/api/event');
		await emitTo('main', QUICK_ADD_CREATED, { kind });
	} catch (e) {
		console.error('quick add: created relay failed', e);
	}
};
