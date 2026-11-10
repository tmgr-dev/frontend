import {
	firesOn,
	frozenUntil,
	governing,
	normalizeDays,
	type Version,
} from '../routines/series';

const v = (over: Partial<Version>): Version => ({
	segmentId: null,
	validFrom: null,
	validTo: null,
	anchor: null,
	recurring: true,
	frequency: 'DAILY',
	interval: 1,
	dayOfFrequency: null,
	month: null,
	daysOfWeek: [],
	scheduledTime: null,
	durationMin: null,
	reminderMin: null,
	title: 't',
	description: null,
	routineCategory: null,
	...over,
});

describe('routine series resolver', () => {
	const seg = v({
		segmentId: 1,
		validFrom: '2026-09-01',
		validTo: '2026-09-10',
		title: 'old',
	});
	const live = v({ validFrom: '2026-09-11', title: 'new' });

	it('finds the last frozen day', () => {
		expect(frozenUntil([])).toBeNull();
		expect(frozenUntil([seg, v({ segmentId: 2, validTo: '2026-09-20' })])).toBe(
			'2026-09-20',
		);
	});

	it('picks the segment for frozen days and the live version after', () => {
		expect(governing([seg], live, '2026-09-05')?.title).toBe('old');
		expect(governing([seg], live, '2026-09-11')?.title).toBe('new');
		expect(governing([], live, '2026-01-01')?.title).toBe('new');
	});

	it('has no governing version in a gap of the frozen range', () => {
		const gap = v({
			segmentId: 2,
			validFrom: '2026-09-05',
			validTo: '2026-09-10',
		});
		const first = v({
			segmentId: 1,
			validFrom: '2026-09-01',
			validTo: '2026-09-02',
		});
		expect(governing([first, gap], live, '2026-09-04')).toBeNull();
	});

	it('fires daily from the anchor on its interval, not from valid_from', () => {
		const every2 = v({
			interval: 2,
			anchor: '2026-09-01',
			validFrom: '2026-09-06',
		});
		expect(firesOn(every2, '2026-09-07', null)).toBe(true);
		expect(firesOn(every2, '2026-09-08', null)).toBe(false);
		expect(firesOn(every2, '2026-09-05', null)).toBe(false);
	});

	it('falls back to valid_from then the fallback anchor', () => {
		expect(firesOn(v({ validFrom: '2026-09-03' }), '2026-09-04', null)).toBe(
			true,
		);
		expect(firesOn(v({}), '2026-09-04', '2026-09-01')).toBe(true);
		expect(firesOn(v({}), '2026-09-04', null)).toBe(false);
	});

	it('does not fire for a non recurring segment', () => {
		expect(
			firesOn(
				v({ recurring: false, anchor: '2026-09-01' }),
				'2026-09-02',
				null,
			),
		).toBe(false);
	});

	it('handles weekly, monthly and yearly phases', () => {
		const weekly = v({
			frequency: 'WEEKLY',
			interval: 2,
			daysOfWeek: ['MON'],
			anchor: '2026-09-07',
		});
		expect(firesOn(weekly, '2026-09-21', null)).toBe(true);
		expect(firesOn(weekly, '2026-09-14', null)).toBe(false);
		const monthly = v({
			frequency: 'MONTHLY',
			interval: 2,
			dayOfFrequency: 5,
			anchor: '2026-01-05',
		});
		expect(firesOn(monthly, '2026-03-05', null)).toBe(true);
		expect(firesOn(monthly, '2026-02-05', null)).toBe(false);
		const yearly = v({
			frequency: 'YEARLY',
			dayOfFrequency: 9,
			month: 2,
			anchor: '2024-03-09',
		});
		expect(firesOn(yearly, '2026-03-09', null)).toBe(true);
		expect(firesOn(yearly, '2026-04-09', null)).toBe(false);
	});

	it('stops at valid_to', () => {
		expect(
			firesOn(
				v({ anchor: '2026-09-01', validTo: '2026-09-05' }),
				'2026-09-06',
				null,
			),
		).toBe(false);
	});

	it('maps two letter weekdays', () => {
		expect(normalizeDays(['mo', 'WE', 'MON'])).toEqual(['MON', 'WED']);
	});
});
