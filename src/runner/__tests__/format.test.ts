import { formatAge } from '../format';

describe('formatAge', () => {
	it.each([
		[0, 'just now'],
		[59_000, 'just now'],
		[60_000, '1m ago'],
		[3_600_000, '1h ago'],
		[90_000_000, '1d ago'],
		[-5000, 'just now'],
	])('%i ms -> %s', (delta, text) => {
		expect(formatAge(1_000_000, 1_000_000 + delta)).toBe(text);
	});
});
