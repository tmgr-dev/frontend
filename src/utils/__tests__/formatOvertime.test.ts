import { formatOvertime } from '../formatOvertime';

describe('formatOvertime', () => {
	it('returns null for non-positive or sub-minute overtime', () => {
		expect(formatOvertime(0)).toBeNull();
		expect(formatOvertime(-120)).toBeNull();
		expect(formatOvertime(30)).toBeNull(); // under a minute
		expect(formatOvertime(NaN)).toBeNull();
	});

	it('shows sub-hour overtime as minutes (not rounded to 0h)', () => {
		expect(formatOvertime(23 * 60)).toBe('23m');
		expect(formatOvertime(59 * 60)).toBe('59m');
	});

	it('shows whole-hour overtime as hours only', () => {
		expect(formatOvertime(3600)).toBe('1h');
		expect(formatOvertime(2 * 3600)).toBe('2h');
	});

	it('shows hours and minutes together', () => {
		expect(formatOvertime(3600 + 23 * 60)).toBe('1h 23m');
		expect(formatOvertime(2 * 3600 + 5 * 60)).toBe('2h 5m');
	});

	it('ignores leftover seconds below a minute', () => {
		expect(formatOvertime(3600 + 23 * 60 + 45)).toBe('1h 23m');
	});
});
