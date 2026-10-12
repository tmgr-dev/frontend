import { consentStore } from '@/privacy/consent';
import { desktopPlatform, isDesktopApp } from '@/utils/desktop';
import { isCatalogRoute, type AccountMode } from './catalog';
import { attachTelemetry } from './attach';
import { createTelemetry } from './client';
import { makeMeta, telemetryBaseUrl } from './env';
import { randomSecret, sha256Hex } from './identity';
import { setTelemetry } from './index';

interface RouterLike {
	afterEach(hook: (to: { name?: unknown }) => void): unknown;
	currentRoute?: { value?: { name?: unknown } };
}

export interface StartOptions {
	router: RouterLike;
	accountMode: () => AccountMode;
}

const desktopVersion = async (): Promise<string> => {
	try {
		const { getVersion } = await import('@tauri-apps/api/app');
		return await getVersion();
	} catch {
		return '';
	}
};

const webVersion = (): string =>
	typeof __APP_VERSION__ === 'string' ? __APP_VERSION__ : '';

export const startTelemetry = async ({ router, accountMode }: StartOptions) => {
	const desktop = isDesktopApp();
	const appVersion = desktop ? await desktopVersion() : webVersion();
	const store = consentStore();
	const telemetry = createTelemetry({
		storage: window.localStorage,
		now: () => Date.now(),
		fetch: (input, init) => window.fetch(input, init),
		baseUrl: telemetryBaseUrl(
			import.meta.env.VITE_TELEMETRY_URL || import.meta.env.VITE_API_BASE_URL,
			window.location.origin,
		),
		consent: {
			read: () => {
				const record = store.read();
				return { on: record.usage === 'granted', generation: record.usageGeneration };
			},
			deny: () => void store.set({ usage: 'denied' }),
		},
		meta: makeMeta({
			win: window,
			desktop: desktop ? desktopPlatform() : false,
			appVersion,
			accountMode,
		}),
		uuid: () => crypto.randomUUID(),
		secret: randomSecret,
		sha256: sha256Hex,
		setInterval: (fn, ms) => window.setInterval(fn, ms),
		clearInterval: (handle) => window.clearInterval(handle as number),
		channel: typeof BroadcastChannel === 'function' ? new BroadcastChannel('tmgr-telemetry') : null,
	});
	setTelemetry(telemetry);
	let opened = false;
	telemetry.subscribe(() => {
		if (opened || !telemetry.snapshot().active) return;
		opened = true;
		telemetry.track('app_opened');
	});
	attachTelemetry(
		telemetry,
		store,
		window,
		desktop
			? (handler) => {
					void import('@tauri-apps/api/window').then(({ getCurrentWindow }) =>
						getCurrentWindow().listen('tauri://close-requested', handler),
					);
				}
			: undefined,
	);
	router.afterEach((to) => {
		if (isCatalogRoute(to.name)) telemetry.track('screen_viewed', { route: to.name });
	});
	const current = router.currentRoute?.value?.name;
	if (isCatalogRoute(current)) telemetry.track('screen_viewed', { route: current });
	return telemetry;
};
