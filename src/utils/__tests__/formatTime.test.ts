import { formatDuration as formatTime } from '../formatDuration';

describe('formatTime', () => {
	it.each([
		[0, '0 minutes'],
		[60, '1 minute'],
		[120, '2 minutes'],
		[3600, '1 hour 0 minutes'],
		[7260, '2 hours 1 minute'],
	])('%s seconds -> %s', (seconds, expected) => {
		expect(formatTime(seconds).replace(/\s+/g, ' ').trim()).toBe(expected);
	});

	it.each([[undefined], [null], [NaN]])('%s -> 0 minutes', (value) => {
		expect(formatTime(value).replace(/\s+/g, ' ').trim()).toBe('0 minutes');
	});
});
