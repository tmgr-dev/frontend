import { formatBoardDate } from '@/utils/boardDate';

describe('formatBoardDate', () => {
	const now = new Date(2026, 7, 30, 15, 0, 0); // Aug 30 2026

	it('labels today and yesterday', () => {
		expect(formatBoardDate('2026-08-30T09:00:00Z', now)).toBe('Today');
		expect(formatBoardDate('2026-08-29T23:30:00', now)).toBe('Yesterday');
	});

	it('falls back to a short month-day for older dates', () => {
		expect(formatBoardDate('2026-01-22T21:13:31Z', now)).toBe('Jan 22');
	});

	it('returns null for missing or unparsable input', () => {
		expect(formatBoardDate(undefined, now)).toBeNull();
		expect(formatBoardDate(null, now)).toBeNull();
		expect(formatBoardDate('not a date', now)).toBeNull();
	});
});
