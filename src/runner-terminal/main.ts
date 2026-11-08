import { detectLocale, setLocale, storedLocale } from '@/i18n';
import { followStoredLocale } from '@/i18n/followStoredLocale';
import { start } from './terminal';

followStoredLocale();

void setLocale(
	detectLocale({ stored: storedLocale(), browser: navigator.languages ?? [] }),
	{ persist: false },
)
	.catch(() => undefined)
	.then(start);
