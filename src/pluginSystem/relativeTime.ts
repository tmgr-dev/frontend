import { t } from '@/i18n';

const MINUTE = 60_000;
const HOUR = 3_600_000;
const DAY = 86_400_000;

const formatSpan = (ms: number): string => {
	if (ms < MINUTE) return t('plugins.helpers.time.fewSeconds');
	if (ms < HOUR)
		return t('plugins.helpers.time.min', { n: Math.round(ms / MINUTE) });
	if (ms < DAY)
		return t('plugins.helpers.time.hour', { n: Math.round(ms / HOUR) });
	return t('plugins.helpers.time.day', { n: Math.round(ms / DAY) });
};

/** "5 min ago" / "in 2 h" / "just now", for a timeAgo node. */
export const formatTimeAgo = (at: string, now: number): string => {
	const diff = now - Date.parse(at);
	if (Math.abs(diff) < MINUTE) return t('plugins.helpers.time.justNow');
	return diff > 0
		? t('plugins.helpers.time.ago', { span: formatSpan(diff) })
		: t('plugins.helpers.time.in', { span: formatSpan(-diff) });
};

export type DueUrgency = 'overdue' | 'soon' | 'later';

/** "overdue 3 h" / "in 2 h" / "in 4 d", plus how urgent it is, for a dueTime node. */
export const formatDueTime = (
	at: string,
	now: number,
): { text: string; urgency: DueUrgency } => {
	const diff = Date.parse(at) - now;
	if (diff < 0)
		return {
			text: t('plugins.helpers.time.overdue', { span: formatSpan(-diff) }),
			urgency: 'overdue',
		};
	if (diff < DAY)
		return {
			text: t('plugins.helpers.time.in', { span: formatSpan(diff) }),
			urgency: 'soon',
		};
	return {
		text: t('plugins.helpers.time.in', { span: formatSpan(diff) }),
		urgency: 'later',
	};
};
