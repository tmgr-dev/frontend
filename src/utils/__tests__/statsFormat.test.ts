import {
	formatCount,
	formatHours,
	hoursToDays,
	hoursToYears,
} from '../statsFormat';

describe('formatCount', () => {
	it('formats a plain integer with locale grouping', () => {
		expect(formatCount(91234)).toBe('91,234');
	});

	it('parses numeric strings', () => {
		expect(formatCount('12')).toBe('12');
	});

	it('renders zero as 0', () => {
		expect(formatCount(0)).toBe('0');
	});

	it('rounds floats', () => {
		expect(formatCount(12.6)).toBe('13');
	});

	it('falls back to an em dash for invalid values', () => {
		expect(formatCount(null)).toBe('—');
		expect(formatCount(undefined)).toBe('—');
		expect(formatCount(NaN)).toBe('—');
		expect(formatCount('not-a-number')).toBe('—');
	});
});

describe('formatHours', () => {
	it('formats with up to one decimal and locale grouping', () => {
		expect(formatHours(91234.5)).toBe('91,234.5');
	});

	it('parses numeric strings', () => {
		expect(formatHours('12')).toBe('12');
	});

	it('renders zero as 0', () => {
		expect(formatHours(0)).toBe('0');
	});

	it('falls back to an em dash for invalid values', () => {
		expect(formatHours(null)).toBe('—');
		expect(formatHours(undefined)).toBe('—');
		expect(formatHours(NaN)).toBe('—');
		expect(formatHours('not-a-number')).toBe('—');
	});
});

describe('hoursToDays', () => {
	it('converts hours to days with two decimals', () => {
		expect(hoursToDays(91234.5)).toBe('3,801.44');
	});

	it('parses numeric strings', () => {
		expect(hoursToDays('24')).toBe('1.00');
	});

	it('falls back to an em dash for invalid values', () => {
		expect(hoursToDays(null)).toBe('—');
		expect(hoursToDays(undefined)).toBe('—');
		expect(hoursToDays(NaN)).toBe('—');
		expect(hoursToDays('not-a-number')).toBe('—');
	});
});

describe('hoursToYears', () => {
	it('converts hours to years with two decimals', () => {
		expect(hoursToYears(8760)).toBe('1.00');
	});

	it('parses numeric strings', () => {
		expect(hoursToYears('8760')).toBe('1.00');
	});

	it('falls back to an em dash for invalid values', () => {
		expect(hoursToYears(null)).toBe('—');
		expect(hoursToYears(undefined)).toBe('—');
		expect(hoursToYears(NaN)).toBe('—');
		expect(hoursToYears('not-a-number')).toBe('—');
	});
});
