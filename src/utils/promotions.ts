export type PromotionPlacement = 'about' | 'release_notes';
export type PromotionEventType = 'impression' | 'click';

export interface Promotion {
	id: number;
	product: string;
	title: string;
	text: string;
	icon_url?: string | null;
	url: string;
	placement: PromotionPlacement;
}

export interface PromotionsDeps {
	isEnabled: () => boolean;
	fetchPromotions: (
		placement: PromotionPlacement,
		locale: string,
	) => Promise<Promotion[]>;
	sendEvent: (
		campaignId: number,
		placement: PromotionPlacement,
		type: PromotionEventType,
	) => Promise<void>;
	openUrl: (url: string) => void;
	locale: () => string;
}

export const createPromotionsController = (deps: PromotionsDeps) => {
	const seen = new Set<string>();

	const load = async (placement: PromotionPlacement): Promise<Promotion[]> => {
		if (!deps.isEnabled()) return [];
		try {
			return await deps.fetchPromotions(placement, deps.locale());
		} catch {
			return [];
		}
	};

	const trackImpression = (promotion: Promotion) => {
		if (!deps.isEnabled()) return;
		const key = `${promotion.id}:${promotion.placement}`;
		if (seen.has(key)) return;
		seen.add(key);
		deps
			.sendEvent(promotion.id, promotion.placement, 'impression')
			.catch(() => seen.delete(key));
	};

	const click = (promotion: Promotion) => {
		deps.sendEvent(promotion.id, promotion.placement, 'click').catch(() => {});
		deps.openUrl(promotion.url);
	};

	return { load, trackImpression, click };
};
