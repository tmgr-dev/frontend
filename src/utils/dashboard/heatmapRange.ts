import { currentLocale, t } from '@/i18n';
import type { HeatmapData, StreakInfo } from '@/types/dashboard';

const DAY_MS = 86_400_000;

const monthName = (month: number) =>
	new Date(Date.UTC(2000, month - 1, 1)).toLocaleDateString(currentLocale(), {
		month: 'short',
		timeZone: 'UTC',
	});

export interface HeatmapRange {
	from: string;
	to: string;
}

export interface HeatmapCell {
	key: string;
	date: string | null; // null = invisible padding outside the range
	count: number;
}

export interface HeatmapMonthLabel {
	key: string;
	label: string;
	column: number;
}

const parseDay = (date: string) => {
	const [y, m, d] = date.split('-').map(Number);
	return Date.UTC(y, m - 1, d);
};
const formatDay = (ms: number) => new Date(ms).toISOString().slice(0, 10);

export const utcToday = (now = new Date()) => now.toISOString().slice(0, 10);

const yearBefore = (day: string) =>
	`${Number(day.slice(0, 4)) - 1}${day.slice(4)}`;

// The old backend sends no from/to: use the contributions' span, else the last year up to today (UTC).
export const resolveHeatmapRange = (
	data: Pick<HeatmapData, 'contributions' | 'from' | 'to'>,
	today: string,
): HeatmapRange => {
	if (data.from && data.to) return { from: data.from, to: data.to };
	const dates = data.contributions.map((c) => c.date).sort();
	if (dates.length) return { from: dates[0], to: dates[dates.length - 1] };
	return { from: yearBefore(today), to: today };
};

export const buildHeatmapWeeks = (
	range: HeatmapRange,
	contributions: HeatmapData['contributions'],
): HeatmapCell[][] => {
	if (range.from > range.to) return [];
	const counts = new Map(contributions.map((c) => [c.date, c.count]));
	const from = parseDay(range.from);
	const to = parseDay(range.to);
	const weeks: HeatmapCell[][] = [];
	let day = from - ((new Date(from).getUTCDay() + 6) % 7) * DAY_MS;
	while (day <= to) {
		const week: HeatmapCell[] = [];
		for (let i = 0; i < 7; i++, day += DAY_MS) {
			const date = formatDay(day);
			week.push(
				day < from || day > to
					? { key: `pad-${date}`, date: null, count: 0 }
					: { key: date, date, count: counts.get(date) ?? 0 },
			);
		}
		weeks.push(week);
	}
	return weeks;
};

// A label needs two columns before the next label or the grid end, else it overlaps or overflows.
export const heatmapMonthLabels = (
	weeks: HeatmapCell[][],
): HeatmapMonthLabel[] => {
	const labels: HeatmapMonthLabel[] = [];
	weeks.forEach((week, column) => {
		const first = week.find((cell) => cell.date)?.date;
		const key = first?.slice(0, 7);
		if (key && key !== labels[labels.length - 1]?.key) {
			labels.push({ key, label: monthName(Number(key.slice(5))), column });
		}
	});
	return labels.filter(
		(label, i) => (labels[i + 1]?.column ?? weeks.length) - label.column > 1,
	);
};

const days = (n: number) => t('board.helpers.heatmap.days', n);

export const heatmapHeadline = (total: number, year: number | null) =>
	year === null
		? t('board.helpers.heatmap.headlineLastYear', total)
		: t('board.helpers.heatmap.headlineYear', { n: total, year }, total);

export const heatmapStreakText = (
	streak: StreakInfo,
	year: number | null,
	today: string,
) =>
	year === null || year === Number(today.slice(0, 4))
		? t('board.helpers.heatmap.streakCurrent', {
				current: days(streak.current),
				longest: days(streak.longest),
			})
		: t('board.helpers.heatmap.streakYear', {
				year,
				longest: days(streak.longest),
			});
