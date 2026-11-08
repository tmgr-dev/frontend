/** @jest-environment jsdom */
const invoke = jest.fn();
let current = 'de';
let listener: ((locale: string) => void) | null = null;

jest.mock('@tauri-apps/api/core', () => ({
	invoke: (...args: unknown[]) => invoke(...args),
}));
jest.mock('@/i18n', () => ({
	t: (key: string) => key,
	currentLocale: () => current,
	onLocaleChange: (fn: (locale: string) => void) => {
		listener = fn;
		return () => undefined;
	},
}));

import { installNativeLocale, pushNativeLocale } from '../desktopTray';

beforeEach(() => {
	invoke.mockReset().mockResolvedValue(undefined);
	listener = null;
	current = 'de';
});

afterEach(() => {
	delete (window as any).__TAURI_INTERNALS__;
});

it('does nothing outside the desktop app', async () => {
	await pushNativeLocale();
	expect(invoke).not.toHaveBeenCalled();
});

it('sends the current locale in the desktop app', async () => {
	(window as any).__TAURI_INTERNALS__ = {};
	await pushNativeLocale();
	expect(invoke).toHaveBeenCalledWith('native_locale_set', { locale: 'de' });
});

it('pushes at install and again on every locale change', async () => {
	(window as any).__TAURI_INTERNALS__ = {};
	installNativeLocale();
	await Promise.resolve();
	await Promise.resolve();
	expect(invoke).toHaveBeenCalledTimes(1);
	current = 'ja';
	listener?.('ja');
	await Promise.resolve();
	await Promise.resolve();
	expect(invoke).toHaveBeenLastCalledWith('native_locale_set', {
		locale: 'ja',
	});
});

it('swallows invoke failures', async () => {
	(window as any).__TAURI_INTERNALS__ = {};
	invoke.mockRejectedValue(new Error('boom'));
	const spy = jest.spyOn(console, 'error').mockImplementation(() => undefined);
	await expect(pushNativeLocale()).resolves.toBeUndefined();
	spy.mockRestore();
});
