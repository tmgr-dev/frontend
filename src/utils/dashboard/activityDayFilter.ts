import { currentLocale } from '@/i18n';

export const toggleDay = (
	current: string | null | undefined,
	clicked: string,
): string | null => (current === clicked ? null : clicked);

// Split instead of new Date('YYYY-MM-DD'): that parses as UTC and shows the previous day west of UTC.
export const formatDayLabel = (date: string): string => {
	const [year, month, day] = date.split('-').map(Number);
	return new Intl.DateTimeFormat(currentLocale(), {
		dateStyle: 'medium',
	}).format(new Date(year, month - 1, day));
};
