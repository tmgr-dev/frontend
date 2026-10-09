import { computed } from 'vue';
import { useStore } from 'vuex';

export function useFeatureToggles() {
	const store = useStore();

	const isFeatureEnabled = (key: string): boolean =>
		store.getters['featureToggles/isFeatureEnabled'](key);

	const getUserFeatureValue = (key: string): string | boolean =>
		store.getters['featureToggles/getUserFeatureValue'](key);

	const isLoaded = computed(() => store.getters['featureToggles/isLoaded']);

	return {
		isWorkspaceFeatureEnabled: isFeatureEnabled,
		isUserFeatureEnabled: isFeatureEnabled,
		getUserFeatureValue,
		isFeatureEnabled,
		isLoaded,
	};
}
