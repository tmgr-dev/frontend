import { requestCache } from '@/utils/requestCache';
import { createI18n, type PluralizationRule } from 'vue-i18n';
import {
	DEFAULT_LOCALE,
	isSupportedLocale,
	LOCALE_STORAGE_KEY,
	pluralIndex,
	SUPPORTED_LOCALES,
	type LocaleCode,
} from './locale';
import { englishMessages, loadMessages } from './messageCatalog';

export * from './locale';

const pluralRules = Object.fromEntries(
	SUPPORTED_LOCALES.map((locale) => [
		locale,
		((choice: number, choicesLength: number) =>
			pluralIndex(locale, choice, choicesLength)) as PluralizationRule,
	]),
);

export const i18n = createI18n({
	legacy: false,
	globalInjection: true,
	locale: DEFAULT_LOCALE,
	fallbackLocale: DEFAULT_LOCALE,
	messages: { [DEFAULT_LOCALE]: englishMessages } as any,
	pluralRules,
	missingWarn: false,
	fallbackWarn: false,
});

export const t = i18n.global.t;

export const currentLocale = (): LocaleCode =>
	i18n.global.locale.value as LocaleCode;

const loaded = new Set<LocaleCode>([DEFAULT_LOCALE]);
let latestRequest = 0;
const listeners = new Set<(locale: LocaleCode) => void>();

export const onLocaleChange = (listener: (locale: LocaleCode) => void) => {
	listeners.add(listener);
	return () => listeners.delete(listener);
};

export const storedLocale = (): string | null => {
	try {
		return localStorage.getItem(LOCALE_STORAGE_KEY);
	} catch {
		return null;
	}
};

export const setLocale = async (
	locale: unknown,
	options: { persist?: boolean } = {},
): Promise<LocaleCode> => {
	const next = isSupportedLocale(locale) ? locale : DEFAULT_LOCALE;
	const ticket = ++latestRequest;
	if (!loaded.has(next)) {
		i18n.global.setLocaleMessage(next, (await loadMessages(next)) as any);
		loaded.add(next);
	}
	if (ticket !== latestRequest) return currentLocale();
	const changed = next !== currentLocale();
	i18n.global.locale.value = next;
	if (typeof document !== 'undefined') document.documentElement.lang = next;
	if (options.persist !== false) {
		try {
			localStorage.setItem(LOCALE_STORAGE_KEY, next);
		} catch {
			/* private mode */
		}
	}
	if (changed) {
		requestCache.clear();
		listeners.forEach((listener) => listener(next));
	}
	return next;
};
