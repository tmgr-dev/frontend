import { useFeatureToggles } from '@/composable/useFeatureToggles';
import { isLocalWorkspaceId } from '@/local/classify';
import { computed } from 'vue';
import { useStore } from 'vuex';

export function useChatsAvailable() {
	const store = useStore();
	const { showSurface } = useFeatureToggles();
	return computed(
		() =>
			store.getters.isLocalOnly !== true &&
			!isLocalWorkspaceId(store.getters.currentWorkspaceId) &&
			showSurface('chats.nav'),
	);
}
