export const DEMO_OFFER_KEY = 'tmgr:onboarding:demo-offered';

export interface DemoOfferFacts {
	localOnly: boolean;
	canUseApp: boolean;
	workspaceCount: number;
	hasDemo: boolean;
	freshInstall: boolean;
	answered: boolean;
}

export const shouldOfferDemo = (facts: DemoOfferFacts): boolean =>
	facts.localOnly &&
	facts.canUseApp &&
	facts.workspaceCount > 0 &&
	!facts.hasDemo &&
	!facts.freshInstall &&
	!facts.answered;

export const demoOfferAnswered = (
	storage: Pick<Storage, 'getItem'>,
): boolean => {
	try {
		return storage.getItem(DEMO_OFFER_KEY) !== null;
	} catch {
		return true;
	}
};

export const rememberDemoOfferAnswer = (
	storage: Pick<Storage, 'setItem'>,
	answer: 'watched' | 'declined',
): void => {
	try {
		storage.setItem(DEMO_OFFER_KEY, answer);
	} catch {
		return;
	}
};
