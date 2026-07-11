import {
	formatRelativeTime,
	isSameTimestamp,
} from '../dailyRoutines/relativeTime';

const NOW = Date.parse('2026-07-11T12:00:00Z');
const at = (secondsAgo: number) =>
	new Date(NOW - secondsAgo * 1000).toISOString();

describe('formatRelativeTime', () => {
	it('returns "now" for moments under 10 seconds ago', () => {
		expect(formatRelativeTime(at(0), NOW)).toBe('now');
		expect(formatRelativeTime(at(9), NOW)).toBe('now');
	});

	it('returns seconds under a minute', () => {
		expect(formatRelativeTime(at(20), NOW)).toBe('20 seconds ago');
		expect(formatRelativeTime(at(59), NOW)).toBe('59 seconds ago');
	});

	it('returns minutes under an hour', () => {
		expect(formatRelativeTime(at(60), NOW)).toBe('1 minute ago');
		expect(formatRelativeTime(at(59 * 60), NOW)).toBe('59 minutes ago');
	});

	it('returns hours under a day', () => {
		expect(formatRelativeTime(at(3600), NOW)).toBe('1 hour ago');
		expect(formatRelativeTime(at(23 * 3600), NOW)).toBe('23 hours ago');
	});

	it('falls back to absolute date+time after a day', () => {
		const label = formatRelativeTime(at(3 * 86400), NOW);
		expect(label).not.toMatch(/ago|now/);
		expect(label).toMatch(/Jul|07/); // locale-dependent month rendering
	});

	it('handles empty and garbage input', () => {
		expect(formatRelativeTime(null, NOW)).toBe('');
		expect(formatRelativeTime(undefined, NOW)).toBe('');
		expect(formatRelativeTime('not-a-date', NOW)).toBe('');
	});
});

describe('isSameTimestamp', () => {
	it('true for equal moments even with different formatting', () => {
		expect(
			isSameTimestamp('2026-07-11T10:00:00Z', '2026-07-11T10:00:00.000Z'),
		).toBe(true);
	});

	it('false for different moments', () => {
		expect(
			isSameTimestamp('2026-07-11T10:00:00Z', '2026-07-11T10:00:01Z'),
		).toBe(false);
	});

	it('handles nulls', () => {
		expect(isSameTimestamp(null, null)).toBe(true);
		expect(isSameTimestamp(null, '2026-07-11T10:00:00Z')).toBe(false);
	});
});
