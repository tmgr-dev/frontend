/**
 * @jest-environment jsdom
 */
import { currentLocale, LOCALE_STORAGE_KEY, setLocale } from '@/i18n';
import { followStoredLocale } from '../followStoredLocale';

const storageEvent = (key: string | null, newValue: string | null) =>
	new StorageEvent('storage', { key, newValue });

describe('followStoredLocale', () => {
	afterEach(async () => {
		await setLocale('en');
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
});
