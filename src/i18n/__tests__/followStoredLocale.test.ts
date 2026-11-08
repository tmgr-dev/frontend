/**
 * @jest-environment jsdom
 */
import { currentLocale, LOCALE_STORAGE_KEY, setLocale } from '@/i18n';
import { followStoredLocale } from '../followStoredLocale';
import * as catalog from '../messageCatalog.node';

const storageEvent = (key: string | null, newValue: string | null) =>
	new StorageEvent('storage', { key, newValue });

const flush = () => new Promise((resolve) => setTimeout(resolve, 20));

describe('followStoredLocale', () => {
	afterEach(async () => {
		jest.restoreAllMocks();
		await setLocale('en');
		localStorage.clear();
	});

	it('switches to the locale written by another window', async () => {
		const stop = followStoredLocale();
		window.dispatchEvent(storageEvent(LOCALE_STORAGE_KEY, 'ru'));
		await new Promise((resolve) => setTimeout(resolve, 0));
		expect(currentLocale()).toBe('ru');
		stop();
	});

	it('ignores other keys and cleared values', async () => {
		const stop = followStoredLocale();
		window.dispatchEvent(storageEvent('other', 'ru'));
		window.dispatchEvent(storageEvent(LOCALE_STORAGE_KEY, null));
		await new Promise((resolve) => setTimeout(resolve, 0));
		expect(currentLocale()).toBe('en');
		stop();
	});

	it('stops listening after the returned disposer runs', async () => {
		followStoredLocale()();
		window.dispatchEvent(storageEvent(LOCALE_STORAGE_KEY, 'ru'));
		await new Promise((resolve) => setTimeout(resolve, 0));
		expect(currentLocale()).toBe('en');
	});

	it('does not write the followed locale back to storage', async () => {
		const stop = followStoredLocale();
		const setItem = jest.spyOn(Storage.prototype, 'setItem');
		window.dispatchEvent(storageEvent(LOCALE_STORAGE_KEY, 'ru'));
		await flush();
		expect(currentLocale()).toBe('ru');
		expect(setItem).not.toHaveBeenCalled();
		stop();
	});

	it('normalizes regional tags and ignores unsupported or unchanged values', async () => {
		const stop = followStoredLocale();
		window.dispatchEvent(storageEvent(LOCALE_STORAGE_KEY, 'ru-RU'));
		await flush();
		expect(currentLocale()).toBe('ru');
		const setItem = jest.spyOn(Storage.prototype, 'setItem');
		window.dispatchEvent(storageEvent(LOCALE_STORAGE_KEY, 'xx'));
		window.dispatchEvent(storageEvent(LOCALE_STORAGE_KEY, 'ru'));
		await flush();
		expect(currentLocale()).toBe('ru');
		expect(setItem).not.toHaveBeenCalled();
		stop();
	});

	it('keeps the newest locale when an older load resolves later', async () => {
		await setLocale('ru');
		const stop = followStoredLocale();
		let release: (value: any) => void = () => undefined;
		const realLoad = catalog.loadMessages;
		jest
			.spyOn(catalog, 'loadMessages')
			.mockImplementationOnce(
				() => new Promise((resolve) => (release = resolve)) as any,
			);
		window.dispatchEvent(storageEvent(LOCALE_STORAGE_KEY, 'ja'));
		window.dispatchEvent(storageEvent(LOCALE_STORAGE_KEY, 'en'));
		await flush();
		expect(currentLocale()).toBe('en');
		release(await realLoad('ja'));
		await flush();
		expect(currentLocale()).toBe('en');
		stop();
	});

	it('swallows a failed catalog load', async () => {
		const stop = followStoredLocale();
		const unhandled = jest.fn();
		process.on('unhandledRejection', unhandled);
		jest.spyOn(catalog, 'loadMessages').mockRejectedValueOnce(new Error('x'));
		window.dispatchEvent(storageEvent(LOCALE_STORAGE_KEY, 'ko'));
		await flush();
		process.off('unhandledRejection', unhandled);
		expect(unhandled).not.toHaveBeenCalled();
		expect(currentLocale()).toBe('en');
		stop();
	});
});
