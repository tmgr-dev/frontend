/** @jest-environment jsdom */
import { createConsentStore } from '../consent';
import { startGoogleAnalytics } from '../googleAnalytics';

const ID = 'G-TEST123';
const scripts = () => document.querySelectorAll('script[src*="googletagmanager"]');

beforeEach(() => {
	localStorage.clear();
	document.head.innerHTML = '';
	delete (window as any).dataLayer;
	delete (window as any)[`ga-disable-${ID}`];
	document.cookie.split(';').forEach((c) => {
		document.cookie = `${c.split('=')[0].trim()}=; expires=Thu, 01 Jan 1970 00:00:00 GMT; path=/`;
	});
});

describe('google analytics loader', () => {
	it('injects nothing until analytics is granted', () => {
		const store = createConsentStore(window);
		startGoogleAnalytics({ id: ID, enabled: true, store });
		expect(scripts()).toHaveLength(0);
		store.set({ analytics: 'denied' });
		expect(scripts()).toHaveLength(0);
		expect((window as any).dataLayer).toBeUndefined();
	});

	it('injects the script with the configured id after a grant', () => {
		const store = createConsentStore(window);
		startGoogleAnalytics({ id: ID, enabled: true, store });
		store.set({ analytics: 'granted' });
		expect(scripts()).toHaveLength(1);
		expect((scripts()[0] as HTMLScriptElement).src).toContain(`id=${ID}`);
		expect((window as any).dataLayer.length).toBe(2);
	});

	it('loads on start when consent was already granted', () => {
		const store = createConsentStore(window);
		store.set({ analytics: 'granted' });
		startGoogleAnalytics({ id: ID, enabled: true, store });
		expect(scripts()).toHaveLength(1);
	});

	it('on revoke removes the script, disables tracking and clears cookies', () => {
		const store = createConsentStore(window);
		startGoogleAnalytics({ id: ID, enabled: true, store });
		store.set({ analytics: 'granted' });
		document.cookie = '_ga=GA1.1.1; path=/';
		document.cookie = '_ga_TEST123=GS1; path=/';
		document.cookie = '_gid=x; path=/';
		document.cookie = 'keep=1; path=/';
		store.set({ analytics: 'denied' });
		expect(scripts()).toHaveLength(0);
		expect((window as any)[`ga-disable-${ID}`]).toBe(true);
		expect(document.cookie).toBe('keep=1');
	});

	it('works again after re-granting in the same page', () => {
		const store = createConsentStore(window);
		startGoogleAnalytics({ id: ID, enabled: true, store });
		store.set({ analytics: 'granted' });
		store.set({ analytics: null });
		expect(scripts()).toHaveLength(0);
		store.set({ analytics: 'granted' });
		expect(scripts()).toHaveLength(1);
		expect((window as any)[`ga-disable-${ID}`]).toBe(false);
	});

	it('never injects when the build is not a web build', () => {
		const store = createConsentStore(window);
		store.set({ analytics: 'granted' });
		startGoogleAnalytics({ id: ID, enabled: false, store });
		store.set({ analytics: 'granted' });
		expect(scripts()).toHaveLength(0);
		expect((window as any).dataLayer).toBeUndefined();
	});

	it('clears cookies left from before and disables tracking when not granted at start', () => {
		document.cookie = '_ga=GA1.1.1.1; path=/';
		const store = createConsentStore(window);
		store.set({ analytics: 'denied' });
		startGoogleAnalytics({ id: ID, enabled: true, store });
		expect(document.cookie).not.toMatch(/_ga=/);
		expect((window as any)[`ga-disable-${ID}`]).toBe(true);
		expect(scripts()).toHaveLength(0);
	});
});

describe('google analytics on auth pages', () => {
	const disabled = () => (window as any)[`ga-disable-${ID}`];
	const granted = () => {
		const store = createConsentStore(window);
		store.set({ analytics: 'granted' });
		return store;
	};

	afterEach(() => window.history.replaceState({}, '', '/'));

	it.each([
		'/login/github?code=x&state=y',
		'/login',
		'/register',
		'/desktop-auth/return',
		'/password/reset?token=t',
	])('loads nothing and sends no config on %s', (url) => {
		window.history.replaceState({}, '', url);
		startGoogleAnalytics({ id: ID, enabled: true, store: granted() });
		expect(scripts()).toHaveLength(0);
		expect((window as any).dataLayer).toBeUndefined();
	});

	it('loads on an app page', () => {
		window.history.replaceState({}, '', '/tmgrdev/board?x=1');
		startGoogleAnalytics({ id: ID, enabled: true, store: granted() });
		expect(scripts()).toHaveLength(1);
		expect((window as any).dataLayer).toHaveLength(2);
	});

	it('loads once the user moves from an auth page to an app page', () => {
		window.history.replaceState({}, '', '/login/github?code=x&state=y');
		let recheck: (pathname: string) => void = () => {};
		startGoogleAnalytics({
			id: ID,
			enabled: true,
			store: granted(),
			onNavigate: (fn) => (recheck = fn),
		});
		expect(scripts()).toHaveLength(0);
		recheck('/tmgrdev/board');
		expect(scripts()).toHaveLength(1);
		expect(disabled()).toBe(false);
	});

	it('stops reporting while on an auth page and resumes after it', () => {
		window.history.replaceState({}, '', '/tmgrdev/board');
		let recheck: (pathname: string) => void = () => {};
		startGoogleAnalytics({
			id: ID,
			enabled: true,
			store: granted(),
			onNavigate: (fn) => (recheck = fn),
		});
		recheck('/login');
		expect(disabled()).toBe(true);
		recheck('/tmgrdev/list');
		expect(disabled()).toBe(false);
		expect(scripts()).toHaveLength(1);
	});

	it('a navigation does not load analytics without consent', () => {
		let recheck: (pathname: string) => void = () => {};
		startGoogleAnalytics({
			id: ID,
			enabled: true,
			store: createConsentStore(window),
			onNavigate: (fn) => (recheck = fn),
		});
		recheck('/tmgrdev/board');
		expect(scripts()).toHaveLength(0);
	});
});
