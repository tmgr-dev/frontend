import { isConfirmNameMatch } from '../confirmName';

describe('isConfirmNameMatch', () => {
	it('matches the exact name', () => {
		expect(isConfirmNameMatch('My WS', 'My WS')).toBe(true);
	});

	it('ignores surrounding spaces in the typed value', () => {
		expect(isConfirmNameMatch('  My WS ', 'My WS')).toBe(true);
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
});
