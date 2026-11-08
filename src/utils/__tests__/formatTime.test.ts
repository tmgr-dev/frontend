import { setLocale } from '@/i18n';
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

	describe('ru plural forms', () => {
		beforeAll(async () => {
			await setLocale('ru', { persist: false });
		});
		afterAll(async () => {
			await setLocale('en', { persist: false });
		});

		it.each([
			[60, '1 минута'],
			[120, '2 минуты'],
			[300, '5 минут'],
			[1260, '21 минута'],
			[3600, '1 час 0 минут'],
			[7200, '2 часа 0 минут'],
			[18000, '5 часов 0 минут'],
			[75600, '21 час 0 минут'],
		])('%s seconds -> %s', (seconds, expected) => {
			expect(formatTime(seconds).replace(/\s+/g, ' ').trim()).toBe(expected);
		});
	});
});
