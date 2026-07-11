/**
 * Human relative timestamp for routine rows: "now" → "N seconds ago" →
 * "N minutes ago" → "N hours ago" → absolute date+time (locale) once the
 * moment is older than a day (or in the future, e.g. clock skew).
 */
export function formatRelativeTime(
	iso: string | null | undefined,
	nowMs: number = Date.now(),
): string {
	if (!iso) return '';
	const t = Date.parse(iso);
	if (Number.isNaN(t)) return '';

	const diffSec = Math.floor((nowMs - t) / 1000);
	if (diffSec < 0) return absolute(t, nowMs);
	if (diffSec < 10) return 'now';
	if (diffSec < 60) return `${diffSec} seconds ago`;
	if (diffSec < 3600) {
		const m = Math.floor(diffSec / 60);
		return m === 1 ? '1 minute ago' : `${m} minutes ago`;
	}
	if (diffSec < 86400) {
		const h = Math.floor(diffSec / 3600);
		return h === 1 ? '1 hour ago' : `${h} hours ago`;
	}
	return absolute(t, nowMs);
}

function absolute(t: number, nowMs: number): string {
	const d = new Date(t);
	const sameYear = d.getFullYear() === new Date(nowMs).getFullYear();
	return d.toLocaleString(undefined, {
		month: 'short',
		day: 'numeric',
		...(sameYear ? {} : { year: 'numeric' }),
		hour: '2-digit',
		minute: '2-digit',
	});
}

/** True when the two ISO timestamps point at the same moment. */
export function isSameTimestamp(
	a: string | null | undefined,
	b: string | null | undefined,
): boolean {
	if (!a || !b) return a === b;
	const ta = Date.parse(a);
	const tb = Date.parse(b);
	if (Number.isNaN(ta) || Number.isNaN(tb)) return a === b;
	return ta === tb;
}
