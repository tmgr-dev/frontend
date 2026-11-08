import { currentLocale, t } from '@/i18n';
import type { RoutineEntry } from '@/types/dailyRoutine';

export interface RecurrenceLike {
	frequency: string;
	interval?: number | null;
	days_of_week?: string[] | null;
	day_of_frequency?: number | null;
	month?: number | null;
}

const DOW_I18N = ['mo', 'tu', 'we', 'th', 'fr', 'sa', 'su'];
const DOW_KEYS = ['MON', 'TUE', 'WED', 'THU', 'FRI', 'SAT', 'SUN'];

function dowKeyToIndex(key: string): number {
	return DOW_KEYS.indexOf(key.toUpperCase());
}

export function recurrenceLabel(r: RecurrenceLike): string {
	if (r.frequency === 'DAILY')
		return (r.interval ?? 1) === 1
			? t('dailyRoutine.helpers.recurrence.daily')
			: t('dailyRoutine.helpers.recurrence.everyNDays', r.interval as number);
	if (r.frequency === 'WEEKLY') {
		const days = (r.days_of_week ?? [])
			.map((d) => dowKeyToIndex(d))
			.filter((i) => i >= 0)
			.sort();
		if (days.length === 0) return t('dailyRoutine.helpers.recurrence.weekly');
		if (days.length === 7) return t('dailyRoutine.helpers.recurrence.daily');
		if (days.join(',') === '0,1,2,3,4')
			return t('dailyRoutine.helpers.recurrence.weekdays');
		if (days.join(',') === '5,6')
			return t('dailyRoutine.helpers.recurrence.weekends');
		return days.map((d) => t(`dailyRoutine.dow.${DOW_I18N[d]}`)).join(' · ');
	}
	if (r.frequency === 'MONTHLY')
		return t('dailyRoutine.helpers.recurrence.monthly', {
			day: r.day_of_frequency ?? 1,
		});
	if (r.frequency === 'YEARLY') {
		const monthIdx = r.month ?? 0;
		const dom = r.day_of_frequency ?? 1;
		return t('dailyRoutine.helpers.recurrence.yearly', {
			date: new Date(2026, monthIdx, dom).toLocaleDateString(currentLocale(), {
				month: 'short',
				day: 'numeric',
			}),
		});
	}
	return '';
}

export function entryRecurrenceLabel(entry: RoutineEntry): string {
	return recurrenceLabel({ frequency: entry.frequency });
}
