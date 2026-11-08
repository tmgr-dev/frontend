import { isValidCode, sanitizeCode } from '../code';

describe('pairing code input', () => {
	it('keeps digits only and caps at six', () => {
		expect(sanitizeCode('12a3-45 6789')).toBe('123456');
		expect(sanitizeCode('abc')).toBe('');
	});

	it('accepts exactly six digits', () => {
		expect(isValidCode('012345')).toBe(true);
		expect(isValidCode('12345')).toBe(false);
		expect(isValidCode('1234567')).toBe(false);
		expect(isValidCode('12345a')).toBe(false);
		expect(isValidCode('123456\n')).toBe(false);
		expect(isValidCode('')).toBe(false);
	});
});
