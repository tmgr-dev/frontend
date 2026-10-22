import {
	daySeparators,
	formatDayLabel,
	formatDuration,
	formatMessageFullTime,
	formatMessageStats,
	formatMessageTime,
} from '../agentMessageMeta';

describe('formatMessageTime', () => {
	it('formats local HH:mm with zero padding', () => {
		const d = new Date(2026, 9, 20, 9, 5);
		expect(formatMessageTime(d.toISOString())).toBe('09:05');
		expect(formatMessageTime(new Date(2026, 9, 20, 14, 32).toISOString())).toBe(
			'14:32',
		);
	});
	it('is empty for pending placeholders and garbage', () => {
		expect(formatMessageTime('')).toBe('');
		expect(formatMessageTime(null)).toBe('');
		expect(formatMessageTime('nope')).toBe('');
	});
});

describe('formatMessageFullTime', () => {
	it('contains the date and time', () => {
		const text = formatMessageFullTime(
			new Date(2026, 9, 20, 14, 32).toISOString(),
		);
		expect(text).toContain('Oct 20, 2026');
		expect(text).toContain('2:32');
	});
	it('is empty when there is no valid date', () => {
		expect(formatMessageFullTime('')).toBe('');
	});
});

describe('formatDayLabel', () => {
	const now = new Date(2026, 9, 22, 12, 0).getTime();
	const at = (y: number, m: number, d: number) =>
		new Date(y, m, d, 23, 59).toISOString();
	it('labels today and yesterday', () => {
		expect(formatDayLabel(at(2026, 9, 22), now)).toBe('Today');
		expect(formatDayLabel(at(2026, 9, 21), now)).toBe('Yesterday');
	});
	it('uses a short date otherwise and adds the year for other years', () => {
		expect(formatDayLabel(at(2026, 9, 20), now)).toBe('Oct 20');
		expect(formatDayLabel(at(2025, 11, 31), now)).toBe('Dec 31, 2025');
	});
	it('is empty for invalid input', () => {
		expect(formatDayLabel('', now)).toBe('');
	});
});

describe('daySeparators', () => {
	const now = new Date(2026, 9, 22, 12, 0).getTime();
	const m = (id: number, d: number, h: number) => ({
		id,
		created_at: new Date(2026, 9, d, h).toISOString(),
	});
	it('is empty when the chat stays within one day', () => {
		expect(daySeparators([m(1, 22, 9), m(2, 22, 10)], now).size).toBe(0);
	});
	it('marks the first message of every day when the chat spans days', () => {
		const result = daySeparators(
			[m(1, 20, 9), m(2, 20, 10), m(3, 21, 8), m(4, 22, 8), m(5, 22, 9)],
			now,
		);
		expect([...result]).toEqual([
			[1, 'Oct 20'],
			[3, 'Yesterday'],
			[4, 'Today'],
		]);
	});
	it('skips messages without a date', () => {
		const result = daySeparators(
			[m(1, 21, 9), { id: 2, created_at: '' }, m(3, 22, 9)],
			now,
		);
		expect([...result.keys()]).toEqual([1, 3]);
	});
});

describe('formatDuration', () => {
	it('shows seconds with one decimal under a minute', () => {
		expect(formatDuration(800)).toBe('0.8 s');
		expect(formatDuration(12400)).toBe('12.4 s');
		expect(formatDuration(0)).toBe('0.0 s');
	});
	it('switches to minutes at 60 s', () => {
		expect(formatDuration(65000)).toBe('1 min 5 s');
		expect(formatDuration(60000)).toBe('1 min');
		expect(formatDuration(59960)).toBe('1 min');
		expect(formatDuration(125400)).toBe('2 min 5 s');
	});
	it('rejects negative and non-finite values', () => {
		expect(formatDuration(-1)).toBe('');
		expect(formatDuration(NaN)).toBe('');
	});
});

describe('formatMessageStats', () => {
	it('joins duration and tokens', () => {
		expect(
			formatMessageStats({
				duration_ms: 12400,
				input_tokens: 3210,
				output_tokens: 450,
			}),
		).toBe('12.4 s · 3,210 in / 450 out tokens');
	});
	it('is empty when every field is null or missing', () => {
		expect(formatMessageStats({})).toBe('');
		expect(
			formatMessageStats({
				duration_ms: null,
				input_tokens: null,
				output_tokens: null,
			}),
		).toBe('');
	});
	it('shows only what is present', () => {
		expect(formatMessageStats({ duration_ms: 800 })).toBe('0.8 s');
		expect(formatMessageStats({ output_tokens: 10 })).toBe('10 out tokens');
		expect(formatMessageStats({ input_tokens: 1000 })).toBe('1,000 in tokens');
	});
	it('keeps zero values', () => {
		expect(formatMessageStats({ duration_ms: 0 })).toBe('0.0 s');
	});
});
