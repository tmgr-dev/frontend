const MINUTE = 60_000;
const HOUR = 3_600_000;
const DAY = 86_400_000;

const formatSpan = (ms: number): string => {
	if (ms < MINUTE) return 'a few seconds';
	if (ms < HOUR) return `${Math.round(ms / MINUTE)} min`;
	if (ms < DAY) return `${Math.round(ms / HOUR)} h`;
	return `${Math.round(ms / DAY)} d`;
};

/** "5 min ago" / "in 2 h" / "just now", for a timeAgo node. */
export const formatTimeAgo = (at: string, now: number): string => {
	const diff = now - Date.parse(at);
	if (Math.abs(diff) < MINUTE) return 'just now';
	return diff > 0 ? `${formatSpan(diff)} ago` : `in ${formatSpan(-diff)}`;
};

export type DueUrgency = 'overdue' | 'soon' | 'later';

/** "overdue 3 h" / "in 2 h" / "in 4 d", plus how urgent it is, for a dueTime node. */
export const formatDueTime = (
	at: string,
	now: number,
): { text: string; urgency: DueUrgency } => {
	const diff = Date.parse(at) - now;
	if (diff < 0) return { text: `overdue ${formatSpan(-diff)}`, urgency: 'overdue' };
	if (diff < DAY) return { text: `in ${formatSpan(diff)}`, urgency: 'soon' };
	return { text: `in ${formatSpan(diff)}`, urgency: 'later' };
};
