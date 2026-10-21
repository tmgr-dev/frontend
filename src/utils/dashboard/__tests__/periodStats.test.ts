import type { TimeDailyDay } from '@/types/dashboard';
import {
	activeMembersCount,
	activeMembersPerDay,
	categoryTotals,
	countsForDays,
	deltaPercent,
	periodLength,
	secondsToHours,
	sumValues,
	trackedSecondsForDays,
	utcDayKeys,
} from '../periodStats';

const today = new Date('2026-10-20T23:30:00Z');

describe('utcDayKeys', () => {
	it('ends on the UTC date of today, oldest first', () => {
		expect(utcDayKeys(today, 7)).toEqual([
			'2026-10-14',
			'2026-10-15',
			'2026-10-16',
			'2026-10-17',
			'2026-10-18',
			'2026-10-19',
			'2026-10-20',
		]);
	});

	it('uses the UTC day even when the local day differs', () => {
		const keys = utcDayKeys(new Date('2026-10-21T00:30:00+02:00'), 1);
		expect(keys).toEqual(['2026-10-20']);
	});

	it('gives the previous equal period with back = length', () => {
		const prev = utcDayKeys(today, 7, 7);
		expect(prev[0]).toBe('2026-10-07');
		expect(prev[6]).toBe('2026-10-13');
	});

	it('spans 30 days across a month boundary', () => {
		const keys = utcDayKeys(today, periodLength('30d'));
		expect(keys).toHaveLength(30);
		expect(keys[0]).toBe('2026-09-21');
		expect(keys[29]).toBe('2026-10-20');
		expect(periodLength('7d')).toBe(7);
	});
});

describe('countsForDays', () => {
	it('slices by date, zero-fills gaps and ignores days outside the period', () => {
		const keys = ['2026-10-18', '2026-10-19', '2026-10-20'];
		expect(
			countsForDays(
				[
					{ date: '2026-10-17', count: 9 },
					{ date: '2026-10-20', count: 4 },
					{ date: '2026-10-18', count: 1 },
				],
				keys,
			),
		).toEqual([1, 0, 4]);
		expect(countsForDays(undefined, keys)).toEqual([0, 0, 0]);
	});

	it('sums previous-period values', () => {
		const points = [
			{ date: '2026-10-07', count: 2 },
			{ date: '2026-10-13', count: 3 },
			{ date: '2026-10-14', count: 50 },
		];
		expect(sumValues(countsForDays(points, utcDayKeys(today, 7, 7)))).toBe(5);
	});
});

describe('deltaPercent', () => {
	it('rounds to a whole percent, positive and negative', () => {
		expect(deltaPercent(15, 10)).toBe(50);
		expect(deltaPercent(5, 10)).toBe(-50);
		expect(deltaPercent(2, 3)).toBe(-33);
		expect(deltaPercent(10, 10)).toBe(0);
	});

	it('has no percent when the previous period is 0', () => {
		expect(deltaPercent(4, 0)).toBeNull();
		expect(deltaPercent(0, 0)).toBeNull();
	});
});

describe('active members', () => {
	const keys = ['2026-10-19', '2026-10-20'];
	const members = [
		{
			daily_activity: [
				{ date: '2026-10-19', count: 2 },
				{ date: '2026-10-20', count: 0 },
			],
		},
		{ daily_activity: [{ date: '2026-10-20', count: 1 }] },
		{ daily_activity: [{ date: '2026-10-18', count: 5 }] },
		{ daily_activity: [] },
	];

	it('counts members with activity per day', () => {
		expect(activeMembersPerDay(members, keys)).toEqual([1, 1]);
	});

	it('counts distinct members active in the period', () => {
		expect(activeMembersCount(members, keys)).toBe(2);
	});
});

const days: TimeDailyDay[] = [
	{
		date: '2026-10-19',
		total_s: 5400,
		items: [
			{
				task: { id: 1, title: 'A' },
				category: { id: 3, code: 'DEV', name: 'Development' },
				seconds: 3600,
			},
			{ task: { id: 2, title: 'B' }, category: null, seconds: 1800 },
		],
	},
	{
		date: '2026-10-20',
		total_s: 9000,
		items: [
			{
				task: { id: 4, title: 'C' },
				category: { id: 5, code: 'DES', name: null },
				seconds: 7200,
			},
			{
				task: { id: 1, title: 'A' },
				category: { id: 3, code: 'DEV', name: 'Development' },
				seconds: 1800,
			},
		],
	},
	{
		date: '2026-10-10',
		total_s: 99999,
		items: [
			{
				task: { id: 9, title: 'old' },
				category: { id: 7, code: 'OLD', name: 'Old' },
				seconds: 99999,
			},
		],
	},
];

describe('tracked time', () => {
	const keys = ['2026-10-18', '2026-10-19', '2026-10-20'];

	it('gives seconds per day, zero-filled', () => {
		expect(trackedSecondsForDays(days, keys)).toEqual([0, 5400, 9000]);
	});

	it('converts seconds to hours with one decimal', () => {
		expect(secondsToHours(5400)).toBe(1.5);
		expect(secondsToHours(1000)).toBe(0.3);
		expect(secondsToHours(0)).toBe(0);
	});

	it('aggregates categories in the period, sorted by time', () => {
		expect(categoryTotals(days, keys)).toEqual([
			{ key: 'c5', name: 'DES', seconds: 7200 },
			{ key: 'c3', name: 'Development', seconds: 5400 },
			{ key: 'none', name: 'No category', seconds: 1800 },
		]);
	});

	it('returns nothing when no time was tracked', () => {
		expect(categoryTotals([], keys)).toEqual([]);
	});

	it('skips empty items, falls back to the id and breaks ties by name', () => {
		const day = (items: TimeDailyDay['items']): TimeDailyDay => ({
			date: '2026-10-20',
			total_s: 0,
			items,
		});
		expect(
			categoryTotals(
				[
					day([
						{
							task: { id: 1, title: 'A' },
							category: { id: 8, code: null, name: null },
							seconds: 600,
						},
						{
							task: { id: 2, title: 'B' },
							category: { id: 9, code: 'B', name: 'Beta' },
							seconds: 600,
						},
						{
							task: { id: 3, title: 'C' },
							category: { id: 10, code: 'Z', name: 'Zero' },
							seconds: 0,
						},
						{
							task: { id: 4, title: 'D' },
							category: { id: 11, code: 'N', name: 'Negative' },
							seconds: -5,
						},
					]),
				],
				keys,
			),
		).toEqual([
			{ key: 'c9', name: 'Beta', seconds: 600 },
			{ key: 'c8', name: 'Category 8', seconds: 600 },
		]);
	});
});

describe('edge cases', () => {
	it('gives -100% when the current period dropped to 0', () => {
		expect(deltaPercent(0, 10)).toBe(-100);
	});

	it('sums duplicate dates and treats non-numeric counts as 0', () => {
		const keys = ['2026-10-19', '2026-10-20'];
		expect(
			countsForDays(
				[
					{ date: '2026-10-20', count: 2 },
					{ date: '2026-10-20', count: 3 },
					{ date: '2026-10-19', count: 'x' as unknown as number },
				],
				keys,
			),
		).toEqual([0, 5]);
	});

	it('handles members without daily_activity', () => {
		const keys = ['2026-10-20'];
		const members = [{ daily_activity: null }, {}];
		expect(activeMembersPerDay(members, keys)).toEqual([0]);
		expect(activeMembersCount(members, keys)).toBe(0);
	});

	it('gives the previous 30 days right before the current 30', () => {
		const prev = utcDayKeys(today, 30, 30);
		expect(prev).toHaveLength(30);
		expect(prev[0]).toBe('2026-08-22');
		expect(prev[29]).toBe('2026-09-20');
	});
});
