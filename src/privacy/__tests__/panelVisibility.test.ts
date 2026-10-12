import { shouldShowConsentPanel } from '../panelVisibility';

const base = { analytics: null, path: '/login', desktop: false } as const;

describe('shouldShowConsentPanel', () => {
	it('shows on the web while no choice is made', () => {
		expect(shouldShowConsentPanel(base)).toBe(true);
		expect(shouldShowConsentPanel({ ...base, path: '/acme/list' })).toBe(true);
	});

	it.each(['granted', 'denied'] as const)('hides once %s', (analytics) => {
		expect(shouldShowConsentPanel({ ...base, analytics })).toBe(false);
	});

	it('hides on the privacy page', () => {
		expect(shouldShowConsentPanel({ ...base, path: '/privacy' })).toBe(false);
		expect(shouldShowConsentPanel({ ...base, path: '/privacy/' })).toBe(false);
		expect(shouldShowConsentPanel({ ...base, path: '/privacy-x' })).toBe(true);
	});

	it('never shows in the desktop app', () => {
		expect(shouldShowConsentPanel({ ...base, desktop: true })).toBe(false);
	});
});
