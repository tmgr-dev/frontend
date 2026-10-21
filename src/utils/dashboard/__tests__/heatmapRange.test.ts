import {
	buildHeatmapWeeks,
	heatmapHeadline,
	heatmapMonthLabels,
	heatmapStreakText,
	resolveHeatmapRange,
} from '../heatmapRange';

const days = (weeks: ReturnType<typeof buildHeatmapWeeks>) =>
	weeks.flat().flatMap((cell) => (cell.date ? [cell.date] : []));

describe('heatmap range (TM-701)', () => {
	it('draws a calendar year up to `to` mid-week with no day after it', () => {
		const weeks = buildHeatmapWeeks({ from: '2026-01-01', to: '2026-10-21' }, [
			{ date: '2026-10-21', count: 3, level: 1 },
		]);
		const dates = days(weeks);
		expect(dates[0]).toBe('2026-01-01');
		expect(dates[dates.length - 1]).toBe('2026-10-21');
		expect(dates).toHaveLength(294);
		expect(weeks.every((week) => week.length === 7)).toBe(true);
		// 2026-01-01 is a Thursday, 2026-10-21 a Wednesday: Mon-start padding on both ends.
		expect(weeks[0].slice(0, 3).map((c) => c.date)).toEqual([null, null, null]);
		expect(weeks[weeks.length - 1].slice(3).every((c) => !c.date)).toBe(true);
		expect(weeks[weeks.length - 1][2]).toMatchObject({
			date: '2026-10-21',
			count: 3,
		});
		expect(new Set(weeks.flat().map((c) => c.key)).size).toBe(weeks.length * 7);
	});

	it('labels a rolling range with the same month twice under unique keys', () => {
		const weeks = buildHeatmapWeeks(
			{ from: '2025-10-21', to: '2026-10-21' },
			[],
		);
		const labels = heatmapMonthLabels(weeks);
		expect(labels.map((l) => l.key)).toEqual([
			'2025-10',
			'2025-11',
			'2025-12',
			'2026-01',
			'2026-02',
			'2026-03',
			'2026-04',
			'2026-05',
			'2026-06',
			'2026-07',
			'2026-08',
			'2026-09',
			'2026-10',
		]);
		expect(labels.filter((l) => l.label === 'Oct')).toHaveLength(2);
		expect(labels[0].column).toBe(0);
		// Nov starts in the week of Mon 2025-11-03.
		expect(weeks[labels[1].column][0].date).toBe('2025-11-03');
	});

	it('labels a calendar year Jan..Dec and drops a month squeezed into one column', () => {
		const year = heatmapMonthLabels(
			buildHeatmapWeeks({ from: '2025-01-01', to: '2025-12-31' }, []),
		);
		expect(year.map((l) => l.label)).toEqual([
			'Jan',
			'Feb',
			'Mar',
			'Apr',
			'May',
			'Jun',
			'Jul',
			'Aug',
			'Sep',
			'Oct',
			'Nov',
			'Dec',
		]);
		// Sat 2025-05-31 alone in the first column, June from the next one.
		const squeezed = heatmapMonthLabels(
			buildHeatmapWeeks({ from: '2025-05-31', to: '2025-07-15' }, []),
		);
		expect(squeezed.map((l) => l.key)).toEqual(['2025-06', '2025-07']);
		// July would start in the last column and overflow the grid's right edge.
		const lastColumn = heatmapMonthLabels(
			buildHeatmapWeeks({ from: '2025-06-02', to: '2025-07-08' }, []),
		);
		expect(lastColumn.map((l) => l.key)).toEqual(['2025-06']);
	});

	it('renders nothing when from > to (Dec 31 edge)', () => {
		const weeks = buildHeatmapWeeks(
			{ from: '2027-01-01', to: '2026-12-31' },
			[],
		);
		expect(weeks).toEqual([]);
		expect(heatmapMonthLabels(weeks)).toEqual([]);
		expect(heatmapHeadline(0, 2027)).toBe('0 contributions in 2027');
	});

	it('falls back to the contributions span, then to the last year (old backend)', () => {
		expect(
			resolveHeatmapRange(
				{
					contributions: [
						{ date: '2025-10-22', count: 1, level: 1 },
						{ date: '2026-10-20', count: 1, level: 1 },
					],
				},
				'2026-10-21',
			),
		).toEqual({ from: '2025-10-22', to: '2026-10-20' });
		expect(resolveHeatmapRange({ contributions: [] }, '2026-10-21')).toEqual({
			from: '2025-10-21',
			to: '2026-10-21',
		});
		expect(
			resolveHeatmapRange(
				{ contributions: [], from: '2025-01-01', to: '2025-12-31' },
				'2026-10-21',
			),
		).toEqual({ from: '2025-01-01', to: '2025-12-31' });
	});

	it('words the headline and streak per view', () => {
		const streak = { current: 1, longest: 4 };
		expect(heatmapHeadline(1, null)).toBe('1 contribution in the last year');
		expect(heatmapHeadline(12, 2025)).toBe('12 contributions in 2025');
		expect(heatmapStreakText(streak, null, '2026-10-21')).toBe(
			'Current streak: 1 day • Longest: 4 days',
		);
		expect(heatmapStreakText(streak, 2026, '2026-10-21')).toBe(
			'Current streak: 1 day • Longest: 4 days',
		);
		expect(heatmapStreakText(streak, 2025, '2026-10-21')).toBe(
			'Longest streak in 2025: 4 days',
		);
	});
});
