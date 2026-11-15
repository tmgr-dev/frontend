import { saveUserLanguage } from '@/actions/tmgr/user';
import { setLocale } from '@/i18n';

export const switchLanguage = async (
	next: string,
	previous: string,
): Promise<boolean> => {
	if (next === previous) return true;
	try {
		const locale = await setLocale(next);
		await saveUserLanguage(locale);
		return true;
	} catch {
		await setLocale(previous).catch(() => undefined);
		return false;
	}
};
