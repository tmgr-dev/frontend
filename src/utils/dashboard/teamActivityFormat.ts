import { t } from '@/i18n';
import type { DailyActivityPoint, TeamActivityWindow } from '@/types/dashboard';

export const TEAM_ACTIVITY_WINDOWS: { key: TeamActivityWindow }[] = [
	{ key: 'today' },
	{ key: '7d' },
	{ key: '30d' },
	{ key: 'all' },
];

/** 0 → "0m", 720 → "12m", 3900 → "1h 05m". */
export function formatTrackedSeconds(sec: number): string {
	const s = Number.isFinite(sec) && sec > 0 ? Math.floor(sec) : 0;
	const h = Math.floor(s / 3600);
	const m = Math.floor((s % 3600) / 60);
	if (h === 0) return t('board.helpers.tracked.minutes', { m });
	return t('board.helpers.tracked.hoursMinutes', {
		h,
		m: m.toString().padStart(2, '0'),
	});
}

/** Pixel heights for a bar sparkline; 0 stays 0, max → maxHeight, others ≥ minHeight. */
export function sparklineBars(
	points: DailyActivityPoint[],
	maxHeight = 24,
	minHeight = 2,
): number[] {
	const max = points.reduce((acc, p) => Math.max(acc, p.count), 0);
	if (max === 0) return points.map(() => 0);
	return points.map((p) =>
		p.count === 0
			? 0
			: Math.max(minHeight, Math.round((p.count / max) * maxHeight)),
	);
}
