import { isUnscheduledDraft, parseTimeStr } from '../dailyRoutines/routineDraft';

describe('isUnscheduledDraft', () => {
	it('keeps an existing one-off routine unscheduled when it only carries the expander date', () => {
		// The expander stamps `date` on every entry (today when unset) and onEdit
		// overlays it as scheduled_date — a date alone is not a user choice.
		expect(
			isUnscheduledDraft({ id: 5, frequency: 'NONE', scheduled_date: '2026-09-07', scheduled_time: null }),
		).toBe(true);
	});

	it('is scheduled when the task carries a time string', () => {
		expect(
			isUnscheduledDraft({ id: 5, frequency: 'NONE', scheduled_date: '2026-09-07', scheduled_time: '09:00' }),
		).toBe(false);
	});

	it('is scheduled when the entry carries a time object', () => {
		expect(isUnscheduledDraft({ id: 5, frequency: 'NONE', time: { h: 9, m: 0 } })).toBe(false);
	});

	it('is never unscheduled for a new draft', () => {
		expect(isUnscheduledDraft({ frequency: 'NONE' })).toBe(false);
	});

	it('is never unscheduled for a recurring routine', () => {
		expect(
			isUnscheduledDraft({ id: 5, recurrence: { frequency: 'WEEKLY', time: { hours: 10, minutes: 30 } } }),
		).toBe(false);
	});
});

describe('parseTimeStr', () => {
	it('parses HH:mm', () => {
		expect(parseTimeStr('18:30')).toEqual({ h: 18, m: 30 });
	});

	it('returns null for non-strings and malformed input', () => {
		expect(parseTimeStr(null)).toBeNull();
		expect(parseTimeStr('nope')).toBeNull();
	});
});
