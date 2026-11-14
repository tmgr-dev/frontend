import { countPeriod, periodBounds } from '../periodCounts';

const e = (date: string, completed = false) => ({ date, completed });

describe('periodBounds', () => {
	const cursor = new Date(2026, 10, 18);

	it('uses the selected day for the day view', () => {
		expect(periodBounds('day', cursor, '2026-11-15')).toEqual({
			from: '2026-11-18',
			to: '2026-11-18',
		});
	});

	it('uses Monday to Sunday for the week view', () => {
		expect(periodBounds('week', cursor, '2026-11-15')).toEqual({
			from: '2026-11-16',
			to: '2026-11-22',
		});
	});

	it('uses the calendar month for the month view', () => {
		expect(periodBounds('month', cursor, '2026-11-15')).toEqual({
			from: '2026-11-01',
			to: '2026-11-30',
		});
	});

	it('falls back to today for list and year', () => {
		const today = { from: '2026-11-15', to: '2026-11-15' };
		expect(periodBounds('list', cursor, '2026-11-15')).toEqual(today);
		expect(periodBounds('year', cursor, '2026-11-15')).toEqual(today);
	});
});

describe('countPeriod', () => {
	it('counts routines and done inside the period only', () => {
		const entries = [
			e('2026-11-17', true),
			e('2026-11-18'),
			e('2026-11-18', true),
			e('2026-11-19', true),
		];

		expect(
			countPeriod(entries, { from: '2026-11-18', to: '2026-11-18' }),
		).toEqual({
			routines: 2,
			completed: 1,
		});
	});
});
