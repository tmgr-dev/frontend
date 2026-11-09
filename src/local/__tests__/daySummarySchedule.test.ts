import {
	DEFAULT_SCHEDULE,
	applyChange,
	decide,
	loadSchedule,
	localDay,
	saveSchedule,
	type ScheduleSettings,
} from '@/local/daySummarySchedule';

const memory = () => {
	const data = new Map<string, string>();
	return {
		getItem: (key: string) => data.get(key) ?? null,
		setItem: (key: string, value: string) => void data.set(key, value),
	};
};

const on = (over: Partial<ScheduleSettings> = {}): ScheduleSettings => ({
	...DEFAULT_SCHEDULE,
	enabled: true,
	enabledAt: new Date(2026, 0, 1, 9, 0).getTime(),
	...over,
});

test('localDay uses the local calendar day', () => {
	expect(localDay(new Date(2026, 2, 8, 23, 59))).toBe('2026-03-08');
	expect(localDay(new Date(2026, 2, 9, 0, 1))).toBe('2026-03-09');
});

test('idle before the time, due-today after it', () => {
	const settings = on();
	expect(decide({ now: new Date(2026, 5, 10, 17, 59), settings })).toEqual({
		kind: 'idle',
	});
	expect(decide({ now: new Date(2026, 5, 10, 18, 0), settings })).toEqual({
		kind: 'due-today',
	});
});

test('already ran today is idle', () => {
	const settings = on({ lastRunDay: '2026-06-10' });
	expect(decide({ now: new Date(2026, 5, 10, 20, 0), settings })).toEqual({
		kind: 'idle',
	});
});

test('disabled is always idle', () => {
	const settings = on({ enabled: false });
	expect(
		decide({ now: new Date(2026, 5, 10, 20, 0), settings, startup: true }),
	).toEqual({ kind: 'idle' });
});

test('startup after a skipped yesterday offers yesterday', () => {
	const settings = on({ lastRunDay: '2026-06-08' });
	const result = decide({
		now: new Date(2026, 5, 10, 9, 0),
		settings,
		startup: true,
	});
	expect(result.kind).toBe('missed');
	expect(result.kind === 'missed' && localDay(result.day)).toBe('2026-06-09');
});

test('missed is only offered at startup', () => {
	const settings = on({ lastRunDay: '2026-06-08' });
	expect(decide({ now: new Date(2026, 5, 10, 9, 0), settings })).toEqual({
		kind: 'idle',
	});
});

test('no missed offer when yesterday ran or the feature was enabled after its time', () => {
	const now = new Date(2026, 5, 10, 9, 0);
	expect(
		decide({
			now,
			startup: true,
			settings: on({ lastRunDay: '2026-06-09' }),
		}).kind,
	).toBe('idle');
	expect(
		decide({
			now,
			startup: true,
			settings: on({ enabledAt: new Date(2026, 5, 9, 19, 0).getTime() }),
		}).kind,
	).toBe('idle');
});

test('no missed offer when today already ran', () => {
	expect(
		decide({
			now: new Date(2026, 5, 10, 19, 0),
			startup: true,
			settings: on({ lastRunDay: '2026-06-10' }),
		}).kind,
	).toBe('idle');
});

test('yesterday is computed on the calendar across a DST change', () => {
	const settings = on({ lastRunDay: '2026-03-01' });
	const result = decide({
		now: new Date(2026, 2, 9, 9, 0),
		settings,
		startup: true,
	});
	expect(result.kind === 'missed' && localDay(result.day)).toBe('2026-03-08');
});

test('invalid time never fires', () => {
	expect(
		decide({ now: new Date(2026, 5, 10, 23, 0), settings: on({ time: 'xx' }) }),
	).toEqual({ kind: 'idle' });
});

test('enabling stamps enabledAt and keeps an unfired day', () => {
	const now = new Date(2026, 5, 10, 10, 0);
	const next = applyChange(
		on({ enabled: false, enabledAt: null }),
		{ enabled: true },
		now,
	);
	expect(next.enabled).toBe(true);
	expect(next.enabledAt).toBe(now.getTime());
});

test('moving the time later after a run lets it fire again today', () => {
	const now = new Date(2026, 5, 10, 19, 0);
	const next = applyChange(
		on({ lastRunDay: '2026-06-10' }),
		{ time: '21:00' },
		now,
	);
	expect(next.lastRunDay).toBeNull();
	expect(decide({ now: new Date(2026, 5, 10, 21, 0), settings: next })).toEqual(
		{
			kind: 'due-today',
		},
	);
});

test('moving the time to one already passed does not fire immediately', () => {
	const now = new Date(2026, 5, 10, 19, 0);
	const next = applyChange(on(), { time: '08:00' }, now);
	expect(decide({ now, settings: next })).toEqual({ kind: 'idle' });
});

test('settings persist and survive garbage', () => {
	const storage = memory();
	expect(loadSchedule(storage)).toEqual(DEFAULT_SCHEDULE);
	saveSchedule(storage, on({ time: '07:30', lastRunDay: '2026-06-09' }));
	expect(loadSchedule(storage)).toEqual(
		on({ time: '07:30', lastRunDay: '2026-06-09' }),
	);
	storage.setItem('tmgr:day-summary-schedule', '{oops');
	expect(loadSchedule(storage)).toEqual(DEFAULT_SCHEDULE);
	storage.setItem(
		'tmgr:day-summary-schedule',
		JSON.stringify({ enabled: 'yes', time: 5 }),
	);
	expect(loadSchedule(storage)).toEqual(DEFAULT_SCHEDULE);
});
