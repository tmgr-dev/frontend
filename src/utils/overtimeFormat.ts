// Formats an overtime duration (in seconds) as a compact "Xh Ym" string.
// Returns '' when there is no positive overtime to show (callers treat the
// empty string as "no overtime"). Minutes are floored; a sub-minute overtime
// therefore yields '' rather than a misleading "0m".
export function formatOvertimeDuration(seconds: number): string {
	if (!seconds || seconds <= 0) {
		return '';
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
	return parts.join(' ');
}
