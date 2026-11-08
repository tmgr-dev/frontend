import { formatDayLabel, toggleDay } from '../activityDayFilter';

test('clicking a day selects it, the same day clears, another day replaces', () => {
	expect(toggleDay(null, '2026-10-12')).toBe('2026-10-12');
	expect(toggleDay('2026-10-12', '2026-10-12')).toBeNull();
	expect(toggleDay('2026-10-12', '2026-10-13')).toBe('2026-10-13');
});

test('day label keeps the calendar date in every time zone', () => {
	expect(formatDayLabel('2026-01-01')).toBe('Jan 1, 2026');
	expect(formatDayLabel('2026-10-12')).toBe('Oct 12, 2026');
	expect(formatDayLabel('2026-12-31')).toBe('Dec 31, 2026');
});
