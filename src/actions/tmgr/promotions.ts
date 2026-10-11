import $axios from '@/plugins/axios';
import type {
	Promotion,
	PromotionEventType,
	PromotionPlacement,
} from '@/utils/promotions';

export const getPromotions = async (
	placement: PromotionPlacement,
	locale: string,
): Promise<Promotion[]> => {
	const { data } = await $axios.get('promotions', {
		params: { placement, locale },
	});
	return Array.isArray(data?.data) ? data.data : [];
};

export const sendPromotionEvent = async (
	campaignId: number,
	placement: PromotionPlacement,
	type: PromotionEventType,
): Promise<void> => {
	await $axios.post('promotions/events', {
		campaign_id: campaignId,
		placement,
		type,
	});
};
