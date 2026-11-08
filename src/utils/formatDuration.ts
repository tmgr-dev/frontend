import { t } from '@/i18n';

export const formatDuration = (seconds: number | null | undefined): string => {
	const total = Number(seconds) || 0;
	const hours = Math.floor(total / 3600);
	const minutes = Math.ceil((total % 3600) / 60);
	const hoursPart = hours > 0 ? `${t('tasks.duration.hours', hours)} ` : '';
	return `${hoursPart}${t('tasks.duration.minutes', minutes)}`;
};
