import type { RoutineEntry } from '@/types/dailyRoutine';
import {
	buildRoutinePayload,
	deleteChoices,
	isStaleSeriesError,
	needsScopeChoice,
	occurrenceRangeOf,
	recurrenceChanged,
	routineDraftFrom,
	routineForEdit,
	scopeOptions,
	type RoutineDraft,
} from '../routinePayload';

const TODAY = '2026-10-12';

const draft = (over: Partial<RoutineDraft> = {}): RoutineDraft => ({
	id: 7,
	title: 'Stretch',
	description: 'Neck too',
	cat: 'health',
	timeH: 8,
	timeM: 15,
	durationMin: 20,
	frequency: 'DAILY',
	interval: 1,
	daysOfWeek: [],
	dayOfMonth: 12,
	month: 0,
	reminderMin: 15,
	reminderAlarm: true,
	unscheduled: false,
	scheduledDate: TODAY,
	...over,
});

const entry = (over: Partial<RoutineEntry> = {}): RoutineEntry => ({
	task_id: 7,
	title: 'Stretch',
	description: null,
	routine_category: { id: 'health', name: 'Health', color: '#22c55e' },
	date: '2026-10-14',
	time: '08:15',
	duration_min: 20,
	reminder_min: 15,
	completed: false,
	status: null,
	instance_id: null,
	virtual: true,
	frequency: 'DAILY',
	scheduled_for: null,
	...over,
});

// GET /daily-routines/tasks/{id}: the bare Task — no pattern, string category.
const fullTask = {
	id: 7,
	title: 'Stretch',
	description: 'Neck too',
	routine_category: 'health',
	approximately_time: 20,
	scheduled_date: null,
	scheduled_time: null,
	recurring: false,
};

describe('buildRoutinePayload', () => {
	it('sends the reminder on the recurrence of a repeating routine', () => {
		const payload = buildRoutinePayload(draft(), TODAY);
		expect(payload.is_recurring).toBe(true);
		expect(payload.recurrence).toMatchObject({
			frequency: 'DAILY',
			reminder_min: 15,
			time: { hours: 8, minutes: 15 },
			duration_min: 20,
		});
	});

	it('sends the phone alarm switch on the recurrence and defaults it on', () => {
		expect(buildRoutinePayload(draft(), TODAY).recurrence.reminder_alarm).toBe(
			true,
		);
		expect(
			buildRoutinePayload(draft({ reminderAlarm: false }), TODAY).recurrence
				.reminder_alarm,
		).toBe(false);
	});

	it('keeps a known interval and sends null when it is unknown', () => {
		expect(
			buildRoutinePayload(draft({ interval: 2 }), TODAY).recurrence.interval,
		).toBe(2);
		expect(
			buildRoutinePayload(draft({ interval: null }), TODAY).recurrence.interval,
		).toBeNull();
	});

	it('an edit or duplicate loaded from the list does not invent interval 1', () => {
		const loaded = routineDraftFrom(
			routineForEdit(fullTask, entry(), []),
			TODAY,
		);
		expect(loaded.interval).toBeNull();
		expect(buildRoutinePayload(loaded, TODAY).recurrence.interval).toBeNull();
	});

	it('keeps an "at time" reminder of 0 minutes', () => {
		expect(
			buildRoutinePayload(draft({ reminderMin: 0 }), TODAY).recurrence
				.reminder_min,
		).toBe(0);
	});

	it('maps weekly days to keys', () => {
		const payload = buildRoutinePayload(
			draft({ frequency: 'WEEKLY', daysOfWeek: [0, 2, 6] }),
			TODAY,
		);
		expect(payload.recurrence.days_of_week).toEqual(['MON', 'WED', 'SUN']);
	});

	it('sends no recurrence (and so no reminder) for a one-off routine', () => {
		const payload = buildRoutinePayload(
			draft({ frequency: 'NONE', scheduledDate: '' }),
			TODAY,
		);
		expect(payload).toMatchObject({
			is_recurring: false,
			scheduled_date: TODAY,
			scheduled_time: { hours: 8, minutes: 15 },
			approximately_time: 20,
		});
		expect(payload.recurrence).toBeUndefined();
		expect(JSON.stringify(payload)).not.toContain('reminder');
	});

	it('marks an unscheduled routine without date, time or recurrence', () => {
		const payload = buildRoutinePayload(
			draft({ frequency: 'NONE', unscheduled: true }),
			TODAY,
		);
		expect(payload.unscheduled).toBe(true);
		expect(payload.recurrence).toBeUndefined();
		expect(payload.scheduled_date).toBeUndefined();
	});
});

describe('routineForEdit + routineDraftFrom (reopening Edit)', () => {
	const reopen = (e: RoutineEntry, week: RoutineEntry[] = []) =>
		routineDraftFrom(routineForEdit(fullTask, e, week), TODAY);

	it('shows the saved reminder, frequency and category of a daily routine', () => {
		const d = reopen(entry());
		expect(d.frequency).toBe('DAILY');
		expect(d.reminderMin).toBe(15);
		expect(d.cat).toBe('health');
		expect([d.timeH, d.timeM, d.durationMin]).toEqual([8, 15, 20]);
	});

	it('keeps an "at time" reminder of 0', () => {
		expect(reopen(entry({ reminder_min: 0 })).reminderMin).toBe(0);
	});

	it('reopens a saved Work / Wednesday / 5m weekly routine', () => {
		const sales = (date: string) =>
			entry({
				task_id: 9934,
				title: 'Sales',
				routine_category: { id: 'work', name: 'Work', color: '#5b8cff' },
				date,
				time: '13:30',
				duration_min: 30,
				reminder_min: 5,
				frequency: 'WEEKLY',
			});
		const bare = {
			...fullTask,
			id: 9934,
			title: 'Sales',
			routine_category: 'work',
		};
		const d = routineDraftFrom(
			routineForEdit(bare, sales('2026-10-14'), [
				sales('2026-10-14'),
				sales('2026-10-21'),
			]),
			TODAY,
		);
		expect(d.cat).toBe('work');
		expect(d.frequency).toBe('WEEKLY');
		expect(d.daysOfWeek).toEqual([2]);
		expect(d.reminderMin).toBe(5);
	});

	it('recovers weekly days from the routine occurrences in that week', () => {
		const d = reopen(entry({ frequency: 'WEEKLY', date: '2026-10-14' }), [
			entry({ date: '2026-10-12' }),
			entry({ date: '2026-10-16' }),
			entry({ task_id: 99, date: '2026-10-17' }),
		]);
		expect(d.daysOfWeek).toEqual([0, 2, 4]);
	});

	it('takes the monthly day and yearly month from the occurrence date', () => {
		expect(
			reopen(entry({ frequency: 'MONTHLY', date: '2026-11-03' })).dayOfMonth,
		).toBe(3);
		const yearly = reopen(entry({ frequency: 'YEARLY', date: '2027-03-09' }));
		expect([yearly.dayOfMonth, yearly.month]).toEqual([9, 2]);
	});

	it('reopens a one-off as scheduled with no reminder', () => {
		const d = reopen(entry({ frequency: 'NONE', reminder_min: null }));
		expect(d.frequency).toBe('NONE');
		expect(d.unscheduled).toBe(false);
		expect(d.scheduledDate).toBe('2026-10-14');
		expect(d.reminderMin).toBeNull();
	});

	it('reads the string category of the bare GET task', () => {
		expect(routineDraftFrom(fullTask, TODAY).cat).toBe('health');
	});

	it('reopens a routine with the phone alarm on unless the server says off', () => {
		expect(reopen(entry()).reminderAlarm).toBe(true);
		const off = routineDraftFrom(
			routineForEdit(
				{ ...fullTask, recurrence: { reminder_alarm: false } },
				entry(),
				[],
			),
			TODAY,
		);
		expect(off.reminderAlarm).toBe(false);
	});

	it('asks for a scope when only the phone alarm switch changed', () => {
		expect(needsScopeChoice(draft({ reminderAlarm: false }), draft())).toBe(
			true,
		);
	});

	it('round-trips the reminder through save', () => {
		const payload = buildRoutinePayload(reopen(entry()), TODAY);
		expect(payload.recurrence.reminder_min).toBe(15);
		expect(payload.routine_category).toBe('health');
	});
});

describe('editing a weekly routine without touching its repeat (D-003)', () => {
	const weekly = (date: string, over: Partial<RoutineEntry> = {}) =>
		entry({ frequency: 'WEEKLY', date, ...over });
	const edit = (e: RoutineEntry, occurrences: RoutineEntry[]) => {
		const original = routineDraftFrom(
			routineForEdit(fullTask, e, occurrences),
			TODAY,
		);
		return {
			original,
			draft: { ...original, daysOfWeek: [...original.daysOfWeek] },
		};
	};

	// Mon+Thu routine that started on Wed 2026-10-14: its first week has only Thu.
	const monThuFromWed = [
		weekly('2026-10-15'),
		weekly('2026-10-19'),
		weekly('2026-10-22'),
		weekly('2026-10-26'),
		weekly('2026-10-29'),
	];

	it('Case A: shows Mon+Thu when opened from the first week, which has only Thu', () => {
		const { original } = edit(monThuFromWed[0], monThuFromWed);
		expect(original.daysOfWeek).toEqual([0, 3]);
	});

	it('Case A: a title-only edit sends no recurrence, so the pattern is kept', () => {
		const { original, draft } = edit(monThuFromWed[0], []);
		draft.title = 'Stretch more';
		const payload = buildRoutinePayload(draft, TODAY, original);
		expect(payload.is_recurring).toBe(true);
		expect(payload.title).toBe('Stretch more');
		expect(payload).not.toHaveProperty('recurrence');
	});

	it('Case B: an occurrence moved from Monday to Wednesday does not make the routine Wednesday-only', () => {
		const moved = weekly('2026-10-14', { virtual: false, instance_id: 5 });
		const { original, draft } = edit(moved, [
			moved,
			weekly('2026-10-19'),
			weekly('2026-10-26'),
			weekly('2026-11-02'),
		]);
		expect(original.daysOfWeek).toContain(0);
		draft.title = 'Stretch more';
		expect(buildRoutinePayload(draft, TODAY, original)).not.toHaveProperty(
			'recurrence',
		);
	});

	it('sends the full recurrence once a recurrence field changes', () => {
		const { original, draft } = edit(monThuFromWed[0], monThuFromWed);
		draft.reminderMin = 30;
		expect(
			buildRoutinePayload(draft, TODAY, original).recurrence,
		).toMatchObject({
			frequency: 'WEEKLY',
			days_of_week: ['MON', 'THU'],
			reminder_min: 30,
		});
	});

	it('treats re-toggled weekdays in another order as unchanged', () => {
		const { original, draft } = edit(monThuFromWed[0], monThuFromWed);
		draft.daysOfWeek = [3, 0];
		expect(buildRoutinePayload(draft, TODAY, original)).not.toHaveProperty(
			'recurrence',
		);
	});
});

describe('occurrenceRangeOf', () => {
	it('spans the Monday–Sunday week of a date and the four weeks after it', () => {
		expect(occurrenceRangeOf('2026-10-14')).toEqual({
			from: '2026-10-12',
			to: '2026-11-15',
		});
		expect(occurrenceRangeOf('2026-10-18')).toEqual({
			from: '2026-10-12',
			to: '2026-11-15',
		});
	});
});

describe('series scope', () => {
	const original = draft();

	it('single scope sends the occurrence time and never the recurrence', () => {
		const p = buildRoutinePayload(
			draft({ title: 'Stretch+', timeH: 9, durationMin: 45 }),
			TODAY,
			original,
			'single',
		);
		expect(p.recurrence).toBeUndefined();
		expect(p.scheduled_time).toEqual({ hours: 9, minutes: 15 });
		expect(p.title).toBe('Stretch+');
		expect(p.approximately_time).toBe(45);
	});

	it('following scope still sends a changed recurrence', () => {
		const p = buildRoutinePayload(
			draft({ frequency: 'WEEKLY', daysOfWeek: [0] }),
			TODAY,
			original,
			'following',
		);
		expect(p.recurrence.frequency).toBe('WEEKLY');
		expect(p.scheduled_time).toBeUndefined();
	});

	it('asks for a scope only for versioned changes of a recurring routine', () => {
		expect(needsScopeChoice(draft({ title: 'X' }), original)).toBe(true);
		expect(needsScopeChoice(draft({ reminderMin: 5 }), original)).toBe(true);
		expect(needsScopeChoice(draft(), original)).toBe(false);
		expect(needsScopeChoice(draft({ id: null, title: 'X' }), original)).toBe(
			false,
		);
		const oneOff = draft({ frequency: 'NONE' });
		expect(
			needsScopeChoice(draft({ title: 'X', frequency: 'NONE' }), oneOff),
		).toBe(false);
		expect(needsScopeChoice(draft({ frequency: 'WEEKLY' }), oneOff)).toBe(
			false,
		);
	});

	it('disables the single option only when the recurrence changed', () => {
		expect(recurrenceChanged(draft({ timeH: 10 }), original)).toBe(false);
		expect(recurrenceChanged(draft({ interval: 2 }), original)).toBe(true);
		expect(scopeOptions(true).find((o) => o.scope === 'single')?.disabled).toBe(
			true,
		);
		expect(scopeOptions(false).every((o) => !o.disabled)).toBe(true);
	});

	it('offers three delete choices for a recurring routine and warns about comments on the whole one', () => {
		expect(deleteChoices(true, 3)).toEqual([
			{ scope: 'single', warnComments: false },
			{ scope: 'following', warnComments: false },
			{ scope: 'all', warnComments: true },
		]);
		expect(deleteChoices(true, 0).some((c) => c.warnComments)).toBe(false);
		expect(deleteChoices(false, 2)).toEqual([
			{ scope: 'all', warnComments: true },
		]);
	});

	it('recognises a stale series conflict', () => {
		expect(isStaleSeriesError({ response: { status: 409 } })).toBe(true);
		expect(isStaleSeriesError({ response: { status: 422 } })).toBe(false);
	});

	it('shows the occurrence override and keeps series_version when editing', () => {
		const entry = {
			task_id: 7,
			date: '2026-10-05',
			time: '08:15',
		} as RoutineEntry;
		const r = routineForEdit(
			{
				id: 7,
				title: 'Stretch',
				series_version: 3,
				occurrence: { title: 'One-off' },
			},
			entry,
		);
		expect(r.title).toBe('One-off');
		expect(r.series_version).toBe(3);
	});
});
