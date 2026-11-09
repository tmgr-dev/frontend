import { doneUnscheduledIds } from '../dailyRoutines/archiveDone';

const entry = (
	task_id: number,
	over: Partial<{
		time: string | null;
		completed: boolean;
		frequency: string | null;
	}>,
) => ({
	task_id,
	time: null as string | null,
	completed: true,
	frequency: 'NONE',
	...over,
});

describe('doneUnscheduledIds', () => {
	it('returns completed unscheduled one-off routines', () => {
		expect(doneUnscheduledIds([entry(1, {}), entry(2, {})] as any)).toEqual([
			1, 2,
		]);
	});

	it('treats a missing frequency as one-off', () => {
		expect(
			doneUnscheduledIds([
				entry(1, { frequency: null }),
				entry(2, { frequency: undefined }),
			] as any),
		).toEqual([1, 2]);
	});

	it('skips recurring, scheduled and open entries', () => {
		expect(
			doneUnscheduledIds([
				entry(1, { frequency: 'DAILY' }),
				entry(2, { time: '09:00' }),
				entry(3, { completed: false }),
				entry(4, {}),
			] as any),
		).toEqual([4]);
	});

	it('deduplicates task ids', () => {
		expect(doneUnscheduledIds([entry(5, {}), entry(5, {})] as any)).toEqual([
			5,
		]);
	});
});
