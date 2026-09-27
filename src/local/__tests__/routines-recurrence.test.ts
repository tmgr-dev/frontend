import {
	addDays,
	daysBetween,
	expandDates,
	firesOn,
	localDate,
	localDateTime,
	nextInstance,
	normalizeRecurrence,
	resolveCategory,
	type RecurrencePattern,
} from '../routines/recurrence';

const basePattern = (
	overrides: Partial<RecurrencePattern> = {},
): RecurrencePattern => ({
	frequency: 'DAILY',
	interval: 1,
	day_of_frequency: null,
	month: null,
	days_of_week: [],
	start_date: null,
	end_date: null,
	occurrences: null,
	scheduled_time: null,
	duration_min: null,
	reminder_min: null,
	...overrides,
});

describe('addDays / daysBetween', () => {
	it('adds and subtracts days across month/year boundaries', () => {
		expect(addDays('2026-05-13', 1)).toBe('2026-05-14');
		expect(addDays('2026-05-31', 1)).toBe('2026-06-01');
		expect(addDays('2025-12-31', 1)).toBe('2026-01-01');
		expect(addDays('2026-03-01', -1)).toBe('2026-02-28');
	});

	it('counts whole days between two dates, negative when reversed', () => {
		expect(daysBetween('2026-05-01', '2026-05-13')).toBe(12);
		expect(daysBetween('2026-05-13', '2026-05-01')).toBe(-12);
		expect(daysBetween('2026-05-13', '2026-05-13')).toBe(0);
	});
});

describe('localDate / localDateTime', () => {
	it('reads device-local wall-clock fields, never UTC-shifted, near local midnight', () => {
		const nearMidnight = new Date(2026, 0, 15, 23, 30, 0);
		expect(localDate(nearMidnight)).toBe('2026-01-15');
		expect(localDateTime(nearMidnight)).toBe('2026-01-15T23:30:00');

		const justAfterMidnight = new Date(2026, 0, 16, 0, 5, 9);
		expect(localDate(justAfterMidnight)).toBe('2026-01-16');
		expect(localDateTime(justAfterMidnight)).toBe('2026-01-16T00:05:09');
	});
});

describe('firesOn — DAILY', () => {
	it('fires every day when interval is 1', () => {
		const p = basePattern({ frequency: 'DAILY', interval: 1 });
		expect(firesOn(p, '2026-05-13', '2026-05-13')).toBe(true);
		expect(firesOn(p, '2026-05-20', '2026-05-13')).toBe(true);
	});

	it('fires every Nth day from the start date with interval > 1', () => {
		const p = basePattern({
			frequency: 'DAILY',
			interval: 3,
			start_date: '2026-05-01',
		});
		expect(firesOn(p, '2026-05-01', '2026-05-01')).toBe(true);
		expect(firesOn(p, '2026-05-02', '2026-05-01')).toBe(false);
		expect(firesOn(p, '2026-05-04', '2026-05-01')).toBe(true);
		expect(firesOn(p, '2026-05-07', '2026-05-01')).toBe(true);
	});

	it('uses rangeFrom as the phase anchor when there is no start date', () => {
		const p = basePattern({
			frequency: 'DAILY',
			interval: 2,
			start_date: null,
		});
		expect(firesOn(p, '2026-05-05', '2026-05-01')).toBe(true);
		expect(firesOn(p, '2026-05-05', '2026-05-02')).toBe(false);
	});

	it('never fires before its start date or after its end date (inclusive bounds)', () => {
		const p = basePattern({
			frequency: 'DAILY',
			interval: 1,
			start_date: '2026-05-10',
			end_date: '2026-05-12',
		});
		expect(firesOn(p, '2026-05-09', '2026-05-01')).toBe(false);
		expect(firesOn(p, '2026-05-10', '2026-05-01')).toBe(true);
		expect(firesOn(p, '2026-05-12', '2026-05-01')).toBe(true);
		expect(firesOn(p, '2026-05-13', '2026-05-01')).toBe(false);
	});
});

describe('firesOn — WEEKLY', () => {
	it('fires only on the configured days of week', () => {
		const p = basePattern({
			frequency: 'WEEKLY',
			interval: 1,
			days_of_week: ['MON', 'WED'],
		});
		expect(firesOn(p, '2026-05-11', '2026-05-11')).toBe(true); // Mon
		expect(firesOn(p, '2026-05-12', '2026-05-11')).toBe(false); // Tue
		expect(firesOn(p, '2026-05-13', '2026-05-11')).toBe(true); // Wed
	});

	it('never fires when days_of_week is empty', () => {
		const p = basePattern({
			frequency: 'WEEKLY',
			interval: 1,
			days_of_week: [],
		});
		expect(firesOn(p, '2026-05-11', '2026-05-11')).toBe(false);
	});

	it('honours interval > 1 (biweekly), phased from the start-of-week of the start date', () => {
		const p = basePattern({
			frequency: 'WEEKLY',
			interval: 2,
			days_of_week: ['MON'],
			start_date: '2026-05-11', // Monday, week 0
		});
		expect(firesOn(p, '2026-05-11', '2026-05-11')).toBe(true); // week 0
		expect(firesOn(p, '2026-05-18', '2026-05-11')).toBe(false); // week 1
		expect(firesOn(p, '2026-05-25', '2026-05-11')).toBe(true); // week 2
	});

	it('handles weeks crossing a year boundary', () => {
		const p = basePattern({
			frequency: 'WEEKLY',
			interval: 2,
			days_of_week: ['THU'],
			start_date: '2025-12-18', // Thursday, week 0
		});
		expect(firesOn(p, '2025-12-18', '2025-12-18')).toBe(true); // week 0
		expect(firesOn(p, '2026-01-01', '2025-12-18')).toBe(true); // week 2, crosses new year
		expect(firesOn(p, '2025-12-25', '2025-12-18')).toBe(false); // week 1
	});
});

describe('firesOn — MONTHLY', () => {
	it('fires on the configured day of month, respecting interval', () => {
		const p = basePattern({
			frequency: 'MONTHLY',
			interval: 2,
			day_of_frequency: 15,
			start_date: '2026-01-15',
		});
		expect(firesOn(p, '2026-01-15', '2026-01-15')).toBe(true);
		expect(firesOn(p, '2026-02-15', '2026-01-15')).toBe(false);
		expect(firesOn(p, '2026-03-15', '2026-01-15')).toBe(true);
	});

	it('31st-of-month never fires in months with fewer than 31 days', () => {
		const p = basePattern({
			frequency: 'MONTHLY',
			interval: 1,
			day_of_frequency: 31,
			start_date: '2026-01-31',
		});
		const dates = expandDates(p, '2026-01-01', '2026-03-31');
		expect(dates).toEqual(['2026-01-31', '2026-03-31']);
	});
});

describe('firesOn — YEARLY', () => {
	it('fires on the configured month/day each year, respecting interval', () => {
		const p = basePattern({
			frequency: 'YEARLY',
			interval: 1,
			day_of_frequency: 25,
			month: 11, // 0-indexed December
			start_date: '2025-12-25',
		});
		expect(firesOn(p, '2025-12-25', '2025-12-25')).toBe(true);
		expect(firesOn(p, '2026-12-25', '2025-12-25')).toBe(true);
		expect(firesOn(p, '2026-11-25', '2025-12-25')).toBe(false);
	});

	it('Feb 29 yearly pattern only fires in leap years', () => {
		const p = basePattern({
			frequency: 'YEARLY',
			interval: 1,
			day_of_frequency: 29,
			month: 1, // 0-indexed February
			start_date: '2028-02-29',
		});
		const dates = expandDates(p, '2027-01-01', '2029-12-31');
		expect(dates).toEqual(['2028-02-29']);
	});
});

describe('expandDates', () => {
	it('is inclusive of both endpoints', () => {
		const p = basePattern({ frequency: 'DAILY', interval: 1 });
		expect(expandDates(p, '2026-05-10', '2026-05-12')).toEqual([
			'2026-05-10',
			'2026-05-11',
			'2026-05-12',
		]);
	});
});

// Ported from NextInstanceCalculatorTest.java. Anchors: 2026-05-11=Mon, 05-12=Tue, 05-13=Wed.
describe('nextInstance', () => {
	const pattern = (freq: string, interval: number | null): RecurrencePattern =>
		basePattern({
			frequency: freq,
			interval: interval ?? 1,
			scheduled_time: '09:00:00',
		});

	it('daily_withLastInstance_addsOneDayAtScheduledTime', () => {
		const p = pattern('DAILY', 1);
		expect(nextInstance(p, '2026-05-12T09:00:00', '2026-05-13T08:00:00')).toBe(
			'2026-05-13T09:00:00',
		);
	});

	it('daily_beyond24hHorizon_returnsEmpty', () => {
		const p = pattern('DAILY', 1);
		expect(
			nextInstance(p, '2026-05-13T09:00:00', '2026-05-13T08:00:00'),
		).toBeNull();
	});

	it('firstOccurrence_beforeScheduledTime_isToday', () => {
		const p = pattern('DAILY', 1);
		expect(nextInstance(p, null, '2026-05-13T08:00:00')).toBe(
			'2026-05-13T09:00:00',
		);
	});

	it('firstOccurrence_afterScheduledTime_isTomorrow', () => {
		const p = pattern('DAILY', 1);
		expect(nextInstance(p, null, '2026-05-13T10:00:00')).toBe(
			'2026-05-14T09:00:00',
		);
	});

	it('weekly_firstOccurrence_onMatchingDayToday', () => {
		const p = { ...pattern('WEEKLY', 1), days_of_week: ['MON', 'WED'] };
		expect(nextInstance(p, null, '2026-05-13T08:00:00')).toBe(
			'2026-05-13T09:00:00',
		);
	});

	it('weekly_nonMatchingDay_nextMatchBeyondHorizon_returnsEmpty', () => {
		const p = { ...pattern('WEEKLY', 1), days_of_week: ['MON', 'WED'] };
		expect(nextInstance(p, null, '2026-05-12T08:00:00')).toBeNull();
	});

	it('monthly_withLastInstance_addsOneMonth', () => {
		const p = pattern('MONTHLY', 1);
		expect(nextInstance(p, '2026-04-13T09:00:00', '2026-05-13T08:00:00')).toBe(
			'2026-05-13T09:00:00',
		);
	});

	it('yearly_withLastInstance_addsOneYear_ignoringMonthColumn', () => {
		const p = { ...pattern('YEARLY', 1), month: 0 };
		expect(nextInstance(p, '2025-05-13T09:00:00', '2026-05-13T08:00:00')).toBe(
			'2026-05-13T09:00:00',
		);
	});

	it('startAtInFuture_returnsEmpty', () => {
		const p = { ...pattern('DAILY', 1), start_date: '2026-06-01' };
		expect(nextInstance(p, null, '2026-05-13T08:00:00')).toBeNull();
	});

	it('endAtPassed_returnsEmpty', () => {
		const p = { ...pattern('DAILY', 1), end_date: '2026-05-12' };
		expect(
			nextInstance(p, '2026-05-12T09:00:00', '2026-05-13T08:00:00'),
		).toBeNull();
	});

	it('nullInterval_defaultsToOne', () => {
		const p = { ...pattern('DAILY', 1), interval: null as unknown as number };
		expect(nextInstance(p, '2026-05-12T09:00:00', '2026-05-13T08:00:00')).toBe(
			'2026-05-13T09:00:00',
		);
	});

	it('invalidFrequency_returnsEmpty', () => {
		const p = pattern('HOURLY', 1);
		expect(
			nextInstance(p, '2026-05-12T09:00:00', '2026-05-13T08:00:00'),
		).toBeNull();
	});

	it('monthly clamps to the last valid day of a shorter month (Jan 31 + 1 month)', () => {
		const p = pattern('MONTHLY', 1);
		expect(nextInstance(p, '2026-01-31T09:00:00', '2026-02-28T08:00:00')).toBe(
			'2026-02-28T09:00:00',
		);
	});
});

// Ported from PatternRequestTest.java resolveScheduledTime() cases, via normalizeRecurrence.
describe('normalizeRecurrence', () => {
	it('returns null when frequency is missing, blank, or NONE', () => {
		expect(normalizeRecurrence(null)).toBeNull();
		expect(normalizeRecurrence(undefined)).toBeNull();
		expect(normalizeRecurrence({})).toBeNull();
		expect(normalizeRecurrence({ frequency: '' })).toBeNull();
		expect(normalizeRecurrence({ frequency: 'NONE' })).toBeNull();
		expect(normalizeRecurrence({ frequency: 'none' })).toBeNull();
	});

	it('defaults interval to 1 when absent', () => {
		expect(normalizeRecurrence({ frequency: 'DAILY' })?.interval).toBe(1);
		expect(
			normalizeRecurrence({ frequency: 'DAILY', interval: 3 })?.interval,
		).toBe(3);
	});

	it('resolves scheduled_time from a {hours,minutes} object', () => {
		expect(
			normalizeRecurrence({
				frequency: 'DAILY',
				time: { hours: 8, minutes: 30 },
			})?.scheduled_time,
		).toBe('08:30:00');
	});

	it('resolves scheduled_time from a {hours,minutes,seconds} object', () => {
		expect(
			normalizeRecurrence({
				frequency: 'DAILY',
				time: { hours: 8, minutes: 30, seconds: 15 },
			})?.scheduled_time,
		).toBe('08:30:15');
	});

	it('resolves scheduled_time from an "HH:mm" string', () => {
		expect(
			normalizeRecurrence({ frequency: 'DAILY', time: '14:45' })
				?.scheduled_time,
		).toBe('14:45:00');
	});

	it('resolves scheduled_time from an "HH:mm:ss" string', () => {
		expect(
			normalizeRecurrence({ frequency: 'DAILY', time: '08:05:09' })
				?.scheduled_time,
		).toBe('08:05:09');
	});

	it('falls back to scheduled_time field when time is absent', () => {
		expect(
			normalizeRecurrence({ frequency: 'DAILY', scheduled_time: '07:15' })
				?.scheduled_time,
		).toBe('07:15:00');
	});

	it('resolves scheduled_time to null when unparseable or absent', () => {
		expect(
			normalizeRecurrence({ frequency: 'DAILY', time: 'not-a-time' })
				?.scheduled_time,
		).toBe(null);
		expect(
			normalizeRecurrence({ frequency: 'DAILY' })?.scheduled_time,
		).toBeNull();
	});

	it('accepts days_of_week and uppercases entries', () => {
		expect(
			normalizeRecurrence({ frequency: 'WEEKLY', days_of_week: ['mon', 'wed'] })
				?.days_of_week,
		).toEqual(['MON', 'WED']);
	});

	it('accepts start_date/end_date under start_at/end_at/dtstart/dtend aliases', () => {
		expect(
			normalizeRecurrence({
				frequency: 'DAILY',
				start_at: '2026-05-01T00:00:00',
			})?.start_date,
		).toBe('2026-05-01');
		expect(
			normalizeRecurrence({ frequency: 'DAILY', dtstart: '2026-05-01' })
				?.start_date,
		).toBe('2026-05-01');
		expect(
			normalizeRecurrence({ frequency: 'DAILY', end_at: '2026-05-10T23:59:00' })
				?.end_date,
		).toBe('2026-05-10');
		expect(
			normalizeRecurrence({ frequency: 'DAILY', dtend: '2026-05-10' })
				?.end_date,
		).toBe('2026-05-10');
	});

	it('passes through month, occurrences, day_of_frequency, duration_min, reminder_min', () => {
		const p = normalizeRecurrence({
			frequency: 'YEARLY',
			month: 5,
			occurrences: 10,
			day_of_frequency: 20,
			duration_min: 45,
			reminder_min: 15,
		});
		expect(p?.month).toBe(5);
		expect(p?.occurrences).toBe(10);
		expect(p?.day_of_frequency).toBe(20);
		expect(p?.duration_min).toBe(45);
		expect(p?.reminder_min).toBe(15);
	});
});

describe('resolveCategory', () => {
	it('matches the frontend ROUTINE_CATEGORIES map', () => {
		expect(resolveCategory('work')).toEqual({
			id: 'work',
			name: 'Work',
			color: '#5b8cff',
		});
		expect(resolveCategory('health')).toEqual({
			id: 'health',
			name: 'Health',
			color: '#22c55e',
		});
		expect(resolveCategory('learn')).toEqual({
			id: 'learn',
			name: 'Learn',
			color: '#a78bfa',
		});
		expect(resolveCategory('home')).toEqual({
			id: 'home',
			name: 'Home',
			color: '#f5b54a',
		});
		expect(resolveCategory('social')).toEqual({
			id: 'social',
			name: 'Social',
			color: '#ec4899',
		});
	});

	it('falls back to General for null, blank, or unknown ids', () => {
		const general = { id: 'none', name: 'General', color: '#888888' };
		expect(resolveCategory(null)).toEqual(general);
		expect(resolveCategory(undefined)).toEqual(general);
		expect(resolveCategory('')).toEqual(general);
		expect(resolveCategory('unknown')).toEqual(general);
	});
});
