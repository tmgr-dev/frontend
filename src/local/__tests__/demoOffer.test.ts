import {
	DEMO_OFFER_KEY,
	demoOfferAnswered,
	rememberDemoOfferAnswer,
	shouldOfferDemo,
	type DemoOfferFacts,
} from '../demoOffer';

const eligible: DemoOfferFacts = {
	localOnly: true,
	canUseApp: true,
	workspaceCount: 1,
	hasDemo: false,
	freshInstall: false,
	answered: false,
};

const memory = () => {
	const map = new Map<string, string>();
	return {
		map,
		getItem: (key: string) => map.get(key) ?? null,
		setItem: (key: string, value: string) => void map.set(key, value),
	};
};

describe('demo offer', () => {
	it('is offered once to an existing local user', () => {
		expect(shouldOfferDemo(eligible)).toBe(true);
	});

	it.each([
		['signed in', { localOnly: false }],
		['app not ready', { canUseApp: false }],
		['no workspaces yet', { workspaceCount: 0 }],
		['demo exists', { hasDemo: true }],
		['fresh install', { freshInstall: true }],
		['already answered', { answered: true }],
	])('is not offered when %s', (_, change) => {
		expect(shouldOfferDemo({ ...eligible, ...change })).toBe(false);
	});

	it('remembers a refusal so the offer never returns', () => {
		const storage = memory();
		expect(demoOfferAnswered(storage)).toBe(false);
		rememberDemoOfferAnswer(storage, 'declined');
		expect(storage.map.get(DEMO_OFFER_KEY)).toBe('declined');
		expect(demoOfferAnswered(storage)).toBe(true);
	});
});
