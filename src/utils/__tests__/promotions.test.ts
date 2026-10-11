import {
	createPromotionsController,
	type Promotion,
	type PromotionsDeps,
} from '../promotions';

const promo = (over: Partial<Promotion> = {}): Promotion => ({
	id: 7,
	product: 'Acme',
	title: 'Try Acme',
	text: 'A tool',
	icon_url: null,
	url: 'https://example.com/?utm_source=tmgr',
	placement: 'about',
	...over,
});

const setup = (over: Partial<PromotionsDeps> = {}) => {
	const deps: PromotionsDeps = {
		isEnabled: () => true,
		fetchPromotions: jest.fn().mockResolvedValue([promo()]),
		sendEvent: jest.fn().mockResolvedValue(undefined),
		openUrl: jest.fn(),
		locale: () => 'en-US',
		...over,
	};
	return { deps, ctl: createPromotionsController(deps) };
};

describe('promotions controller', () => {
	it('does not fetch when the module is off', async () => {
		const { deps, ctl } = setup({ isEnabled: () => false });
		expect(await ctl.load('about')).toEqual([]);
		expect(deps.fetchPromotions).not.toHaveBeenCalled();
	});

	it('fetches with placement and navigator locale when on', async () => {
		const { deps, ctl } = setup();
		expect(await ctl.load('release_notes')).toEqual([promo()]);
		expect(deps.fetchPromotions).toHaveBeenCalledWith('release_notes', 'en-US');
	});

	it('returns nothing when the request fails', async () => {
		const { ctl } = setup({
			fetchPromotions: jest.fn().mockRejectedValue(new Error('403')),
		});
		expect(await ctl.load('about')).toEqual([]);
	});

	it('sends one impression per campaign and placement', () => {
		const { deps, ctl } = setup();
		ctl.trackImpression(promo());
		ctl.trackImpression(promo());
		ctl.trackImpression(promo({ placement: 'release_notes' }));
		ctl.trackImpression(promo({ id: 8 }));
		expect(deps.sendEvent).toHaveBeenCalledTimes(3);
		expect(deps.sendEvent).toHaveBeenCalledWith(7, 'about', 'impression');
	});

	it('sends no impression when the module is off', () => {
		const { deps, ctl } = setup({ isEnabled: () => false });
		ctl.trackImpression(promo());
		expect(deps.sendEvent).not.toHaveBeenCalled();
	});

	it('allows a retry after a failed impression', async () => {
		const sendEvent = jest
			.fn()
			.mockRejectedValueOnce(new Error('net'))
			.mockResolvedValue(undefined);
		const { ctl } = setup({ sendEvent });
		ctl.trackImpression(promo());
		await Promise.resolve();
		await Promise.resolve();
		ctl.trackImpression(promo());
		expect(sendEvent).toHaveBeenCalledTimes(2);
	});

	it('records a click without waiting and opens the url', () => {
		const sendEvent = jest.fn().mockReturnValue(new Promise(() => {}));
		const { deps, ctl } = setup({ sendEvent });
		ctl.click(promo());
		expect(sendEvent).toHaveBeenCalledWith(7, 'about', 'click');
		expect(deps.openUrl).toHaveBeenCalledWith(promo().url);
	});

	it('still opens the url when the click event fails', async () => {
		const { deps, ctl } = setup({
			sendEvent: jest.fn().mockRejectedValue(new Error('x')),
		});
		ctl.click(promo());
		await Promise.resolve();
		expect(deps.openUrl).toHaveBeenCalledTimes(1);
	});
});
