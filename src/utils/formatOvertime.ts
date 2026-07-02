/**
 * Formats an overtime amount (seconds a task ran over its estimate) as a compact
 * hours+minutes string — e.g. 4980s -> "1h 23m", 1380s -> "23m", 3600s -> "1h".
 *
 * Returns null when there is no meaningful overtime (<= 0, or under a minute), so callers
 * can hide the overtime affordance entirely. Sub-hour overtime stays visible as minutes
 * rather than being rounded to "0.0h".
 */
export function formatOvertime(seconds: number): string | null {
	if (!Number.isFinite(seconds) || seconds <= 0) {
		return null;
	}
	const hours = Math.floor(seconds / 3600);
	const minutes = Math.floor((seconds % 3600) / 60);
	const parts: string[] = [];
	if (hours > 0) {
		parts.push(`${hours}h`);
	}
	if (minutes > 0) {
		parts.push(`${minutes}m`);
	}
	return parts.length > 0 ? parts.join(' ') : null;
}
