jest.mock('@/plugins/axios', () => ({
	__esModule: true,
	default: { get: jest.fn(), put: jest.fn() },
}));
jest.mock('@/store', () => ({
	__esModule: true,
	default: {
		state: { sessionGeneration: 1 },
		getters: { userSettingByKey: () => undefined, isLoggedIn: true },
		commit: jest.fn(),
	},
}));
jest.mock('@/utils/requestCache', () => ({
	__esModule: true,
	default: { getOrFetch: jest.fn() },
}));
jest.mock('@/utils/timezoneDetect', () => ({ detectUserTimeZone: jest.fn() }));
jest.mock('@/i18n', () => ({
	currentLocale: jest.fn(),
	isSupportedLocale: jest.requireActual('@/i18n/locale').isSupportedLocale,
	setLocale: jest.fn(),
}));

import { currentLocale, setLocale } from '@/i18n';
import $axios from '@/plugins/axios';
import store from '@/store';
import requestCache from '@/utils/requestCache';
import { getUser, saveUserLanguage } from '../user';

const settle = () => new Promise((resolve) => setTimeout(resolve, 0));

const withLanguage = (entry: { key: string; value: unknown } | undefined) => {
	(store.getters as any).userSettingByKey = (key: string) =>
		key === 'language' ? entry : undefined;
};

beforeEach(() => {
	jest.clearAllMocks();
	(store.getters as any).isLoggedIn = true;
	(currentLocale as jest.Mock).mockReturnValue('en');
	(setLocale as jest.Mock).mockResolvedValue('en');
	($axios.put as jest.Mock).mockResolvedValue({
		data: { data: { language: 'en', changed: true } },
	});
	(requestCache.getOrFetch as jest.Mock).mockResolvedValue({
		id: 2,
		settings: [],
	});
});

describe('getUser applies the account language', () => {
	test('a supported account language is applied without a PUT', async () => {
		withLanguage({ key: 'language', value: 'ru' });
		await getUser();
		await settle();
		expect(setLocale).toHaveBeenCalledWith('ru');
		expect($axios.put).not.toHaveBeenCalled();
	});

	test.each([null, ''])(
		'an empty account language (%p) seeds the account from the current locale',
		async (value) => {
			(currentLocale as jest.Mock).mockReturnValue('es');
			withLanguage({ key: 'language', value });
			await getUser();
			await settle();
			expect($axios.put).toHaveBeenCalledWith('user/language', {
				language: 'es',
				if_unset: true,
			});
			expect(setLocale).not.toHaveBeenCalled();
		},
	);

	test('an unsupported account language is treated as unset', async () => {
		withLanguage({ key: 'language', value: 'xx' });
		await getUser();
		await settle();
		expect($axios.put).toHaveBeenCalledWith('user/language', {
			language: 'en',
			if_unset: true,
		});
		expect(setLocale).not.toHaveBeenCalled();
	});

	test('a missing language entry does nothing', async () => {
		withLanguage(undefined);
		await getUser();
		await settle();
		expect(setLocale).not.toHaveBeenCalled();
		expect($axios.put).not.toHaveBeenCalled();
	});

	test('a failing seed request does not break getUser', async () => {
		withLanguage({ key: 'language', value: null });
		($axios.put as jest.Mock).mockRejectedValue(new Error('offline'));
		await expect(getUser()).resolves.toEqual({ id: 2, settings: [] });
		await settle();
		expect($axios.put).toHaveBeenCalledTimes(1);
	});
});

describe('saveUserLanguage', () => {
	test('sends the chosen language', async () => {
		withLanguage({ key: 'language', value: 'en' });
		await expect(saveUserLanguage('ru')).resolves.toBeUndefined();
		expect($axios.put).toHaveBeenCalledWith('user/language', {
			language: 'ru',
		});
	});

	test('rejects when the request fails', async () => {
		withLanguage({ key: 'language', value: 'en' });
		const failure = new Error('500');
		($axios.put as jest.Mock).mockRejectedValue(failure);
		await expect(saveUserLanguage('ru')).rejects.toBe(failure);
	});

	test('sends nothing when the account has no language entry', async () => {
		withLanguage(undefined);
		await saveUserLanguage('ru');
		expect($axios.put).not.toHaveBeenCalled();
	});

	test('sends nothing when logged out', async () => {
		withLanguage({ key: 'language', value: 'en' });
		(store.getters as any).isLoggedIn = false;
		await saveUserLanguage('ru');
		expect($axios.put).not.toHaveBeenCalled();
	});
});
