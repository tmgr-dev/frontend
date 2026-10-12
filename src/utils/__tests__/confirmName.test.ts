import { isConfirmNameMatch } from '../confirmName';

describe('isConfirmNameMatch', () => {
	it('matches the exact name', () => {
		expect(isConfirmNameMatch('My WS', 'My WS')).toBe(true);
	});

	it('treats surrounding spaces in the typed value as a mismatch', () => {
		expect(isConfirmNameMatch('  My WS ', 'My WS')).toBe(false);
	});

	it('is case sensitive', () => {
		expect(isConfirmNameMatch('my ws', 'My WS')).toBe(false);
	});

	it('rejects a partial match', () => {
		expect(isConfirmNameMatch('My', 'My WS')).toBe(false);
	});

	it('rejects everything when the expected name is blank', () => {
		expect(isConfirmNameMatch('', '')).toBe(false);
		expect(isConfirmNameMatch('  ', '  ')).toBe(false);
	});

	it('rejects empty typed value', () => {
		expect(isConfirmNameMatch('', 'My WS')).toBe(false);
	});

	it('needs the exact characters of a name with spaces around it', () => {
		expect(isConfirmNameMatch('My WS', ' My WS ')).toBe(false);
		expect(isConfirmNameMatch(' My WS ', ' My WS ')).toBe(true);
		expect(isConfirmNameMatch('\u00a0My WS', '\u00a0My WS')).toBe(true);
	});
});
