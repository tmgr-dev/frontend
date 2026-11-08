import { currentLocale, t } from '@/i18n';
import { isValid } from 'date-fns';

/** Compact date label for board cards: "Today" / "Yesterday" / "Jan 22". */
export function formatBoardDate(
	dateStr: string | null | undefined,
	now: Date = new Date(),
): string | null {
	if (!dateStr) return null;
	const d = new Date(dateStr);
	if (!isValid(d)) return null;
	const sameDay = (a: Date, b: Date) =>
		a.getFullYear() === b.getFullYear() &&
		a.getMonth() === b.getMonth() &&
		a.getDate() === b.getDate();
	const yesterday = new Date(now);
	yesterday.setDate(now.getDate() - 1);
	if (sameDay(d, now)) return t('tasks.helpers.boardDate.today');
	if (sameDay(d, yesterday)) return t('tasks.helpers.boardDate.yesterday');
	return d.toLocaleDateString(currentLocale(), {
		month: 'short',
		day: 'numeric',
	});
}
