import ModuleOffNotice from '@/components/general/ModuleOffNotice.vue';
import { useFeatureToggles } from '@/composable/useFeatureToggles';
import { defineAsyncComponent, defineComponent, h, type Component } from 'vue';

export const gatedPage = (
	featureKey: string,
	title: string,
	loader: () => Promise<{ default: Component }>,
) => {
	const Page = defineAsyncComponent(loader);
	return defineComponent({
		name: `ModuleGate(${featureKey})`,
		setup() {
			const { isFeatureEnabled } = useFeatureToggles();
			return () =>
				isFeatureEnabled(featureKey)
					? h(Page)
					: h(ModuleOffNotice, { title, featureKey });
		},
	});
};
