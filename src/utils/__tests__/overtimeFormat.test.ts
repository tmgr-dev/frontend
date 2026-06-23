import { formatOvertimeDuration } from '../overtimeFormat';

describe('formatOvertimeDuration', () => {
	it('returns empty string for no / non-positive overtime', () => {
		expect(formatOvertimeDuration(0)).toBe('');
		expect(formatOvertimeDuration(-120)).toBe('');
		expect(formatOvertimeDuration(NaN)).toBe('');
	});

	it('returns empty string for sub-minute overtime (floored)', () => {
		expect(formatOvertimeDuration(45)).toBe('');
		expect(formatOvertimeDuration(59)).toBe('');
	});

	it('shows minutes only when under an hour', () => {
		expect(formatOvertimeDuration(60)).toBe('1m');
		expect(formatOvertimeDuration(30 * 60)).toBe('30m');
		expect(formatOvertimeDuration(59 * 60)).toBe('59m');
	});

	it('shows hours only when on a whole hour boundary', () => {
		expect(formatOvertimeDuration(3600)).toBe('1h');
		expect(formatOvertimeDuration(2 * 3600)).toBe('2h');
	});

	it('shows hours and minutes together', () => {
		expect(formatOvertimeDuration(3600 + 30 * 60)).toBe('1h 30m');
		expect(formatOvertimeDuration(2 * 3600 + 5 * 60)).toBe('2h 5m');
	});

	it('regression: a sub-hour overtime is no longer rounded away to 0h', () => {
		// Previously rendered as "0.0h" via (seconds / 3600).toFixed(1).
		expect(formatOvertimeDuration(20 * 60)).toBe('20m');
	});
});
