import { LOCALE_STORAGE_KEY, setLocale } from '@/i18n';

export const followStoredLocale = (target: Window = window): (() => void) => {
	const listener = (event: StorageEvent) => {
		if (event.key === LOCALE_STORAGE_KEY && event.newValue) {
			void setLocale(event.newValue);
		}
	};
	target.addEventListener('storage', listener);
	return () => target.removeEventListener('storage', listener);
};
