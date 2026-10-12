/** @jest-environment jsdom */
import { createConsentStore } from '@/privacy/consent';
import { attachTelemetry } from '../attach';
import { makeMeta, telemetryBaseUrl } from '../env';
import { IDENTITY_KEY } from '../identity';
import { FakeChannel, makeTab, setConsent, tick } from './harness';

beforeEach(() => {
	localStorage.clear();
	FakeChannel.all = [];
});
afterEach(tick);

const hide = (state: 'hidden' | 'visible') => {
	Object.defineProperty(document, 'visibilityState', { value: state, configurable: true });
	document.dispatchEvent(new Event('visibilitychange'));
};

describe('window wiring', () => {
	it('flushes with keepalive when the page is hidden, only while sharing is on', async () => {
		setConsent('granted', 1);
		const tab = makeTab();
		const store = createConsentStore(window);
		const detach = attachTelemetry(tab.telemetry, store, window);
		tab.telemetry.track('app_opened');
		hide('visible');
		await tick();
		expect(tab.calls).toEqual([]);
		hide('hidden');
		await tick();
		expect(tab.calls).toHaveLength(1);
		expect(tab.calls[0].init.keepalive).toBe(true);
		tab.telemetry.track('connect_stalled');
		setConsent('denied', 2);
		hide('hidden');
		await tick();
		expect(tab.calls).toHaveLength(1);
		detach();
	});

	it('flushes on a desktop quit signal and respects consent', async () => {
		setConsent('granted', 1);
		const tab = makeTab();
		let quit = () => {};
		attachTelemetry(tab.telemetry, createConsentStore(window), window, (handler) => (quit = handler));
		tab.telemetry.track('app_opened');
		quit();
		await tick();
		expect(tab.calls).toHaveLength(1);
		tab.telemetry.track('connect_stalled');
		setConsent('denied', 2);
		quit();
		await tick();
		expect(tab.calls).toHaveLength(1);
	});

	it('stops on a storage event from another tab and clears the queue', () => {
		setConsent('granted', 1);
		const tab = makeTab();
		attachTelemetry(tab.telemetry, createConsentStore(window), window);
		tab.telemetry.track('app_opened');
		expect(tab.telemetry.snapshot().pending).toBe(1);
		setConsent('denied', 2);
		window.dispatchEvent(new StorageEvent('storage', { key: 'tmgr:consent:v1' }));
		expect(tab.telemetry.snapshot()).toEqual({ active: false, installId: null, pending: 0, error: null });
		expect(localStorage.getItem(IDENTITY_KEY)).toBeNull();
	});

	it('does not start when attached without consent', () => {
		const tab = makeTab();
		attachTelemetry(tab.telemetry, createConsentStore(window), window);
		expect(tab.telemetry.snapshot().active).toBe(false);
		expect(localStorage.getItem(IDENTITY_KEY)).toBeNull();
	});
});

describe('environment', () => {
	const win = (width: number, coarse: boolean) =>
		({ innerWidth: width, matchMedia: () => ({ matches: coarse }) }) as unknown as Window;
	const meta = (width: number, coarse: boolean, desktop: 'macos' | 'windows' | 'linux' | null | false) =>
		makeMeta({ win: win(width, coarse), desktop, appVersion: '3.4.87', accountMode: () => 'none' })();

	it('classifies platform and viewport', () => {
		expect(meta(1280, false, false)).toMatchObject({ platform: 'web', viewport: 'l', app_version: '3.4' });
		expect(meta(390, true, false)).toMatchObject({ platform: 'mobile_web', viewport: 's' });
		expect(meta(390, false, false).platform).toBe('web');
		expect(meta(800, true, false)).toMatchObject({ platform: 'web', viewport: 'm' });
		expect(meta(1280, false, 'windows').platform).toBe('desktop_win');
		expect(meta(1280, false, 'linux').platform).toBe('desktop_linux');
		expect(meta(1280, false, 'macos').platform).toBe('desktop_mac');
		expect(meta(1280, false, null).platform).toBe('desktop_mac');
	});

	it('resolves the endpoint base', () => {
		expect(telemetryBaseUrl('https://api.tmgr.dev/api/', 'https://tmgr.dev')).toBe('https://api.tmgr.dev/api/');
		expect(telemetryBaseUrl('/api', 'https://tmgr.dev')).toBe('https://tmgr.dev/api/');
		expect(telemetryBaseUrl(undefined, 'https://tmgr.dev')).toBe('https://tmgr.dev/api/');
	});
});
