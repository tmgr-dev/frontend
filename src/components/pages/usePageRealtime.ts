import { getSharedEcho, usePusher } from '@/composable/usePusher';
import store from '@/store';
import type { PageUpdatedEvent } from '@/utils/pages/realtime';
import { onBeforeUnmount, onMounted } from 'vue';

export const usePageRealtime = (
	getWorkspaceId: () => number | null | undefined,
	onEvent: (event: PageUpdatedEvent) => void,
) => {
	const pusher = usePusher();
	let workspaceId: number | null = null;
	let subscriptionId = '';
	let listener: ((event: PageUpdatedEvent) => void) | null = null;

	onMounted(() => {
		const id = getWorkspaceId() ?? store.getters.currentWorkspaceId;
		if (!id || id < 0) return;
		subscriptionId = pusher.subscribeToWorkspace(id, {});
		const channel = getSharedEcho()?.private(`App.Workspace.${id}`);
		if (!channel) return;
		workspaceId = id;
		listener = (event) => onEvent(event);
		channel.listen('.page.updated', listener);
	});

	onBeforeUnmount(() => {
		if (workspaceId === null) return;
		if (listener) {
			getSharedEcho()
				?.private(`App.Workspace.${workspaceId}`)
				.stopListening('.page.updated', listener);
		}
		if (subscriptionId) {
			pusher.unsubscribeHandlerFromWorkspace(workspaceId, subscriptionId);
		}
	});
};
