export const SUPPORTED_LOCALES = [
	'en',
	'ru',
	'es',
	'zh-CN',
	'pt-BR',
	'de',
	'fr',
	'ja',
	'ko',
	'pl',
] as const;

export type LocaleCode = (typeof SUPPORTED_LOCALES)[number];

export const DEFAULT_LOCALE: LocaleCode = 'en';

export const LOCALE_STORAGE_KEY = 'tmgr.language';

export const LOCALE_NAMES: Record<LocaleCode, string> = {
	en: 'English',
	ru: 'Русский',
	es: 'Español',
	'zh-CN': '简体中文',
	'pt-BR': 'Português (Brasil)',
	de: 'Deutsch',
	fr: 'Français',
	ja: '日本語',
	ko: '한국어',
	pl: 'Polski',
};

type PluralCategory = Intl.LDMLPluralRule;

export const PLURAL_FORMS: Record<LocaleCode, PluralCategory[]> = {
	en: ['one', 'other'],
	ru: ['one', 'few', 'many'],
	es: ['one', 'other'],
	'zh-CN': ['other'],
	'pt-BR': ['one', 'other'],
	de: ['one', 'other'],
	fr: ['one', 'other'],
	ja: ['other'],
	ko: ['other'],
	pl: ['one', 'few', 'many'],
};

const FALLBACK_CATEGORY: Partial<Record<PluralCategory, PluralCategory>> = {
	zero: 'other',
	two: 'other',
	many: 'other',
};

export const isSupportedLocale = (value: unknown): value is LocaleCode =>
	typeof value === 'string' &&
	(SUPPORTED_LOCALES as readonly string[]).includes(value);

export const normalizeLocale = (tag: unknown): LocaleCode | null => {
	if (typeof tag !== 'string' || tag.trim() === '') return null;
	const parts = tag.trim().replace(/_/g, '-').toLowerCase().split('-');
	const language = parts[0];
	const exact = SUPPORTED_LOCALES.find(
		(code) => code.toLowerCase() === parts.slice(0, 2).join('-'),
	);
	if (exact) return exact;
	if (language === 'zh') {
		const traditional = parts.some((p) =>
			['hant', 'tw', 'hk', 'mo'].includes(p),
		);
		return traditional ? null : 'zh-CN';
	}
	if (language === 'pt') return 'pt-BR';
	return SUPPORTED_LOCALES.find((code) => code === language) ?? null;
};

export interface LocaleSources {
	setting?: unknown;
	stored?: unknown;
	browser?: readonly unknown[];
}

export const detectLocale = ({
	setting,
	stored,
	browser = [],
}: LocaleSources): LocaleCode => {
	for (const candidate of [setting, stored, ...browser]) {
		const locale = normalizeLocale(candidate);
		if (locale) return locale;
	}
	return DEFAULT_LOCALE;
};

const rulesCache = new Map<LocaleCode, Intl.PluralRules>();

export const pluralIndex = (
	locale: LocaleCode,
	choice: number,
	choicesLength: number,
): number => {
	const forms = PLURAL_FORMS[locale];
	if (choicesLength !== forms.length) {
		return Math.min(Math.abs(choice) === 1 ? 0 : 1, choicesLength - 1);
	}
	let rules = rulesCache.get(locale);
	if (!rules) {
		rules = new Intl.PluralRules(locale);
		rulesCache.set(locale, rules);
	}
	const category = rules.select(Math.abs(choice));
	const direct = forms.indexOf(category);
	if (direct >= 0) return direct;
	const mapped = forms.indexOf(FALLBACK_CATEGORY[category] ?? 'other');
	if (mapped >= 0) return mapped;
	return forms.indexOf('few') >= 0 ? forms.indexOf('few') : forms.length - 1;
};
