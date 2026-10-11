import { getPromotions, sendPromotionEvent } from '@/actions/tmgr/promotions';
import { useFeatureToggles } from '@/composable/useFeatureToggles';
import {
	createPromotionsController,
	type Promotion,
	type PromotionPlacement,
} from '@/utils/promotions';
import store from '@/store';
import { onMounted, ref, watch } from 'vue';

const MODULE_KEY = 'promotions';

let controller: ReturnType<typeof createPromotionsController> | null = null;
let enabledNow: () => boolean = () => true;

const shared = () => {
	if (!controller) {
		controller = createPromotionsController({
			isEnabled: () => enabledNow(),
			fetchPromotions: getPromotions,
			sendEvent: sendPromotionEvent,
			openUrl: (url) => {
				window.open(url, '_blank', 'noopener,noreferrer');
			},
			locale: () =>
				(typeof navigator !== 'undefined' && navigator.language) || 'en',
		});
	}
	return controller;
};

export function usePromotions(placement: PromotionPlacement) {
	const { isFeatureEnabled, isLoaded } = useFeatureToggles();
	enabledNow = () =>
		isFeatureEnabled(MODULE_KEY) && store.getters.isLocalOnly !== true;
	const promotions = ref<Promotion[]>([]);
	const ctl = shared();

	const refresh = async () => {
		if (!isLoaded.value) return;
		promotions.value = await ctl.load(placement);
	};

	onMounted(refresh);
	watch([enabledNow, isLoaded], refresh);

	return {
		promotions,
		trackImpression: ctl.trackImpression,
		click: ctl.click,
	};
}
