import { t } from '@/i18n';

export const formatAge = (createdMs: number, nowMs: number): string => {
	const seconds = Math.max(0, Math.floor((nowMs - createdMs) / 1000));
	if (seconds < 60) return t('desktop.helpers.runner.age.justNow');
	const minutes = Math.floor(seconds / 60);
	if (minutes < 60)
		return t('desktop.helpers.runner.age.minutes', { n: minutes });
	const hours = Math.floor(minutes / 60);
	if (hours < 24) return t('desktop.helpers.runner.age.hours', { n: hours });
	return t('desktop.helpers.runner.age.days', { n: Math.floor(hours / 24) });
};
