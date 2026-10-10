import { computed } from 'vue';
import { useStore } from 'vuex';
import { showSurface as surfaceVisible, type SurfaceId } from '@/utils/moduleSurfaces';

export function useFeatureToggles() {
	const store = useStore();

	const isFeatureEnabled = (key: string): boolean =>
		store.getters['featureToggles/isFeatureEnabled'](key);

	const showSurface = (id: SurfaceId): boolean =>
		surfaceVisible(id, isFeatureEnabled);

	const getUserFeatureValue = (key: string): string | boolean =>
		store.getters['featureToggles/getUserFeatureValue'](key);

	const isLoaded = computed(() => store.getters['featureToggles/isLoaded']);

	return {
		isWorkspaceFeatureEnabled: isFeatureEnabled,
		isUserFeatureEnabled: isFeatureEnabled,
		getUserFeatureValue,
		isFeatureEnabled,
		showSurface,
		isLoaded,
	};
}
