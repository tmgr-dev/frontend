import { isAuthPath } from '@/utils/stripAnalytics';
import type { ConsentStore } from './consent';

const SCRIPT_ATTR = 'data-tmgr-ga';
const COOKIE_NAME = /^(_ga|_ga_.+|_gid|_gat.*)$/;

const cookieDomains = (host: string): string[] => {
	const parts = host.split('.');
	const out = [host, `.${host}`];
	for (let i = 1; i < parts.length - 1; i++) {
		const parent = parts.slice(i).join('.');
		out.push(parent, `.${parent}`);
	}
	return out;
};

export const clearGoogleCookies = (doc: Document, host: string) => {
	const names = doc.cookie
		.split(';')
		.map((pair) => pair.split('=')[0].trim())
		.filter((name) => COOKIE_NAME.test(name));
	for (const name of names) {
		const expired = `${name}=; expires=Thu, 01 Jan 1970 00:00:00 GMT; path=/`;
		doc.cookie = expired;
		for (const domain of cookieDomains(host)) {
			doc.cookie = `${expired}; domain=${domain}`;
		}
	}
};

export interface GoogleAnalyticsOptions {
	id: string;
	enabled: boolean;
	store: ConsentStore;
	win?: Window;
	onNavigate?: (recheck: (pathname: string) => void) => void;
}

export const startGoogleAnalytics = ({
	id,
	enabled,
	store,
	win = window,
	onNavigate,
}: GoogleAnalyticsOptions): (() => void) => {
	if (!enabled) return () => {};
	const doc = win.document;
	const flag = `ga-disable-${id}`;
	const w = win as unknown as Record<string, unknown>;
	let loaded = false;

	const load = () => {
		if (loaded) return;
		loaded = true;
		w[flag] = false;
		const dataLayer = (w.dataLayer = (w.dataLayer as unknown[]) || []);
		w.gtag = function () {
			// gtag.js requires the Arguments object, not an array
			dataLayer.push(arguments);
		};
		(w.gtag as (...args: unknown[]) => void)('js', new Date());
		(w.gtag as (...args: unknown[]) => void)('config', id);
		const script = doc.createElement('script');
		script.async = true;
		script.src = `https://www.googletagmanager.com/gtag/js?id=${encodeURIComponent(id)}`;
		script.setAttribute(SCRIPT_ATTR, '');
		doc.head.appendChild(script);
	};

	const revoke = () => {
		loaded = false;
		w[flag] = true;
		doc.querySelectorAll(`script[${SCRIPT_ATTR}]`).forEach((el) => el.remove());
		clearGoogleCookies(doc, win.location.hostname);
	};

	const apply = (
		analytics: string | null,
		pathname = win.location.pathname,
	) => {
		if (analytics !== 'granted') return revoke();
		if (isAuthPath(pathname)) {
			w[flag] = true;
			return;
		}
		w[flag] = false;
		load();
	};

	// Back/Forward changes the URL before router guards run; registered before gtag.js so it runs first.
	const onPopState = () => {
		if (store.get().analytics === 'granted')
			w[flag] = isAuthPath(win.location.pathname);
	};
	win.addEventListener('popstate', onPopState);

	apply(store.get().analytics);
	onNavigate?.((pathname) => {
		if (store.get().analytics === 'granted') apply('granted', pathname);
	});
	const unsubscribe = store.subscribe((record) => apply(record.analytics));
	return () => {
		unsubscribe();
		win.removeEventListener('popstate', onPopState);
	};
};
