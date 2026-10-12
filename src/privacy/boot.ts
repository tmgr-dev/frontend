import router from '@/router';
import { isDesktopApp } from '@/utils/desktop';
import { carriesSecret } from '@/utils/stripAnalytics';

const DEFAULT_GA_ID = 'G-4GXTBLV7EG';

export const startPrivacy = () => {
	if (import.meta.env.MODE !== 'desktop' && !isDesktopApp()) {
		// Plausible reports pushState URLs itself; a full load re-runs the index.html gate that keeps it off these.
		router.beforeEach((to) => {
			if (
				!carriesSecret(to.fullPath) ||
				!document.querySelector('script[data-domain]')
			)
				return true;
			window.location.assign(to.fullPath);
			return false;
		});
		void Promise.all([
			import('./consent'),
			import('./googleAnalytics'),
		]).then(([{ consentStore }, { startGoogleAnalytics }]) =>
			startGoogleAnalytics({
				id: import.meta.env.VITE_GA_ID || DEFAULT_GA_ID,
				enabled: true,
				store: consentStore(),
				// Before the URL changes, so gtag's history page_view never sees an auth URL.
				onNavigate: (recheck) => router.beforeResolve((to) => recheck(to.path)),
			}),
		);
	}
};
