import {
	currentLocale,
	LOCALE_STORAGE_KEY,
	normalizeLocale,
	setLocale,
} from '@/i18n';

export const followStoredLocale = (target: Window = window): (() => void) => {
	const listener = (event: StorageEvent) => {
		if (event.key !== LOCALE_STORAGE_KEY) return;
		const next = normalizeLocale(event.newValue);
		if (!next || next === currentLocale()) return;
		setLocale(next, { persist: false }).catch(() => undefined);
	};
	target.addEventListener('storage', listener);
	return () => target.removeEventListener('storage', listener);
};
