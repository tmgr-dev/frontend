export function parseTimeStr(s: any): { h: number; m: number } | null {
	if (typeof s !== 'string') return null;
	const parts = s.split(':');
	if (parts.length < 2) return null;
	const h = parseInt(parts[0], 10);
	const m = parseInt(parts[1], 10);
	if (isNaN(h) || isNaN(m)) return null;
	return { h, m };
}

/**
 * An existing one-off routine opens the edit modal as "unscheduled" only when it
 * carries no time. A date alone does not count: the expander stamps `date` on every
 * entry (today when unset), so treating it as a user choice silently schedules the
 * routine at the default time on save.
 */
export function isUnscheduledDraft(r: any): boolean {
	const isExisting = r?.id != null;
	const frequency = r?.recurrence?.frequency ?? r?.frequency ?? 'NONE';
	if (!isExisting || frequency !== 'NONE') return false;
	const hasTime =
		r.time?.h != null ||
		r.recurrence?.time?.hours != null ||
		parseTimeStr(r.scheduled_time) !== null;
	return !hasTime;
}
