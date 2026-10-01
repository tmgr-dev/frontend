import { nextTick, ref } from 'vue';

const handlers: Record<string, any> = {};
const subscribeToWorkspace = jest.fn((id: number, h: any) => {
	handlers[id] = h;
	return `sub-${id}`;
});
const unsubscribeHandlerFromWorkspace = jest.fn();
const commit = jest.fn();

jest.mock('@/composable/usePusher', () => ({
	usePusher: () => ({ subscribeToWorkspace, unsubscribeHandlerFromWorkspace }),
}));
jest.mock('@/store', () => ({ __esModule: true, default: { commit, state: { pagesEvent: null } } }));
jest.mock('@/actions/tmgr/pages', () => ({ invalidatePages: jest.fn() }));

import { usePagesRealtime } from '../usePagesRealtime';

it('a page window subscribes once its workspace is ready and turns page events into store events its page view watches', async () => {
	const workspaceId = ref<number | null>(-3);
	const ready = ref(false);
	usePagesRealtime(workspaceId, ready, () => undefined);
	expect(subscribeToWorkspace).not.toHaveBeenCalled();
	ready.value = true;
	await nextTick();
	expect(subscribeToWorkspace).toHaveBeenCalledWith(-3, expect.objectContaining({ onPageEvent: expect.any(Function) }));
	const page = { id: 4, slug: 'doc', version: 3 };
	handlers[-3].onPageEvent('page.updated', { page });
	expect(commit).toHaveBeenCalledWith('pagesEvent', { type: 'page.updated', page });
});
