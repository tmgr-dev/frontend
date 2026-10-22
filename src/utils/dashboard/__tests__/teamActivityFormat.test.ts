import {
	TEAM_ACTIVITY_WINDOWS,
	formatTrackedSeconds,
	sparklineBars,
	teamActivitySubtitle,
	teamActivityWindowLabel,
} from '../teamActivityFormat';

describe('team activity window labels', () => {
	it('lists the four windows in order', () => {
		expect(TEAM_ACTIVITY_WINDOWS.map((w) => w.label)).toEqual([
			'Today',
			'7d',
			'30d',
			'All time',
		]);
	});
	it('labels each window for the summary', () => {
		expect(teamActivityWindowLabel('all')).toBe('All time');
		expect(teamActivityWindowLabel('7d')).toBe('7d');
	});
	it('words the subtitle per window', () => {
		expect(teamActivitySubtitle('today')).toBe('Members’ activity today');
		expect(teamActivitySubtitle('7d')).toBe(
			'Members’ activity in the last 7 days',
		);
		expect(teamActivitySubtitle('30d')).toBe(
			'Members’ activity in the last 30 days',
		);
		expect(teamActivitySubtitle('all')).toBe('Members’ activity, all time');
	});
});

describe('formatTrackedSeconds', () => {
	it('formats zero and minutes', () => {
		expect(formatTrackedSeconds(0)).toBe('0m');
		expect(formatTrackedSeconds(59)).toBe('0m');
		expect(formatTrackedSeconds(12 * 60)).toBe('12m');
	});
	it('formats hours with zero-padded minutes', () => {
		expect(formatTrackedSeconds(3600 + 5 * 60)).toBe('1h 05m');
		expect(formatTrackedSeconds(6 * 3600 + 40 * 60 + 30)).toBe('6h 40m');
		expect(formatTrackedSeconds(27 * 3600 + 3 * 60)).toBe('27h 03m');
	});
	it('treats negative/NaN as zero', () => {
		expect(formatTrackedSeconds(-5)).toBe('0m');
		expect(formatTrackedSeconds(NaN)).toBe('0m');
	});
});

describe('sparklineBars', () => {
	const p = (counts: number[]) =>
		counts.map((count, i) => ({ date: `2026-08-${10 + i}`, count }));
	it('returns zeros for all-empty', () => {
		expect(sparklineBars(p([0, 0, 0]))).toEqual([0, 0, 0]);
	});
	it('scales to max height and clamps small values to min height', () => {
		expect(sparklineBars(p([0, 1, 12, 24]), 24, 2)).toEqual([0, 2, 12, 24]);
	});
	it('handles empty input', () => {
		expect(sparklineBars([])).toEqual([]);
	});
});
