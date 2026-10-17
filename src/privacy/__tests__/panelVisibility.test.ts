import {
	isAuthRoute,
	shouldShowConsentPanel,
	showAnalyticsBlock,
	showUsageBlock,
} from '../panelVisibility';

const base = {
	analytics: null,
	usage: null,
	path: '/login',
	desktop: false,
} as const;

describe('consent panel visibility', () => {
	it('shows both blocks on the web while nothing is answered', () => {
		expect(shouldShowConsentPanel(base)).toBe(true);
		expect(shouldShowConsentPanel({ ...base, path: '/acme/list' })).toBe(true);
		expect(showAnalyticsBlock(base)).toBe(true);
		expect(showUsageBlock(base)).toBe(true);
	});

	it('keeps the panel until every shown block is answered', () => {
		const analyticsAnswered = { ...base, analytics: 'denied' } as const;
		expect(shouldShowConsentPanel(analyticsAnswered)).toBe(true);
		expect(showAnalyticsBlock(analyticsAnswered)).toBe(false);
		const usageAnswered = { ...base, usage: 'granted' } as const;
		expect(shouldShowConsentPanel(usageAnswered)).toBe(true);
		expect(showUsageBlock(usageAnswered)).toBe(false);
		expect(
			shouldShowConsentPanel({
				...base,
				analytics: 'granted',
				usage: 'denied',
			}),
		).toBe(false);
	});

	it('hides on the privacy page', () => {
		expect(shouldShowConsentPanel({ ...base, path: '/privacy' })).toBe(false);
		expect(shouldShowConsentPanel({ ...base, path: '/privacy/' })).toBe(false);
		expect(shouldShowConsentPanel({ ...base, path: '/privacy-x' })).toBe(true);
	});

	it('asks only about usage data in the desktop app', () => {
		const desktop = { ...base, desktop: true } as const;
		expect(showAnalyticsBlock(desktop)).toBe(false);
		expect(showUsageBlock(desktop)).toBe(true);
		expect(shouldShowConsentPanel(desktop)).toBe(true);
		expect(
			shouldShowConsentPanel({ ...desktop, analytics: null, usage: 'denied' }),
		).toBe(false);
	});

	it('waits for the desktop welcome step before asking', () => {
		const desktop = { ...base, desktop: true } as const;
		expect(shouldShowConsentPanel({ ...desktop, ready: false })).toBe(false);
		expect(shouldShowConsentPanel({ ...desktop, ready: true })).toBe(true);
		expect(shouldShowConsentPanel({ ...base, ready: false })).toBe(true);
	});
});

describe('isAuthRoute', () => {
	it('recognises the pages drawn by AuthLayout', () => {
		expect(isAuthRoute('Login')).toBe(true);
		expect(isAuthRoute('Register')).toBe(true);
		expect(isAuthRoute('WorkspaceBoard')).toBe(false);
		expect(isAuthRoute(undefined)).toBe(false);
	});
});
