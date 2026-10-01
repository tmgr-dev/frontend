import { invalidatePages } from '@/actions/tmgr/pages';
import { usePusher } from '@/composable/usePusher';
import store from '@/store';
import type { PageEventPayload, PageEventType } from '@/types/dashboard';
import { onBeforeUnmount, watch, type Ref } from 'vue';

const REFRESH_DEBOUNCE_MS = 150;

export function usePagesRealtime(
	workspaceId: Ref<number | null>,
	enabled: Ref<boolean>,
	refresh: () => void,
) {
	let subscribedTo: number | null = null;
	let subscriptionId = '';
	let timer: ReturnType<typeof setTimeout> | null = null;

	const scheduleRefresh = () => {
		if (timer) clearTimeout(timer);
		timer = setTimeout(() => {
			timer = null;
			refresh();
		}, REFRESH_DEBOUNCE_MS);
	};

	const unsubscribe = () => {
		if (subscribedTo !== null && subscriptionId) {
			usePusher().unsubscribeHandlerFromWorkspace(subscribedTo, subscriptionId);
		}
		subscribedTo = null;
		subscriptionId = '';
	};

	const subscribe = (id: number) => {
		subscriptionId = usePusher().subscribeToWorkspace(id, {
			onPageEvent: (type: PageEventType, payload: PageEventPayload) => {
				invalidatePages();
				store.commit('pagesEvent', { type, page: payload.page });
				scheduleRefresh();
			},
			onReconnect: scheduleRefresh,
		});
		subscribedTo = id;
	};

	watch(
		[workspaceId, enabled],
		([id, on]) => {
			unsubscribe();
			if (id !== null && on) subscribe(id);
		},
		{ immediate: true },
	);

	onBeforeUnmount(() => {
		unsubscribe();
		if (timer) clearTimeout(timer);
	});
}
