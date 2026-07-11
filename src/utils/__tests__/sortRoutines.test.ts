import { sortUnscheduledNewestFirst } from '../dailyRoutines/sortRoutines';

const e = (task_id: number | undefined, created_at?: string | null) =>
	({ task_id, created_at } as any);

describe('sortUnscheduledNewestFirst', () => {
	it('orders by created_at descending (newest first)', () => {
		const sorted = sortUnscheduledNewestFirst([
			e(1, '2026-01-01T10:00:00Z'),
			e(2, '2026-07-11T10:00:00Z'),
			e(3, '2026-03-05T10:00:00Z'),
		]);
		expect(sorted.map(x => x.task_id)).toEqual([2, 3, 1]);
	});

	it('falls back to task_id descending when created_at is missing', () => {
		const sorted = sortUnscheduledNewestFirst([e(5), e(90), e(12)]);
		expect(sorted.map(x => x.task_id)).toEqual([90, 12, 5]);
	});

	it('does not scramble the list when a single entry lacks created_at', () => {
		const sorted = sortUnscheduledNewestFirst([
			e(1, '2026-01-01T10:00:00Z'),
			e(99, null), // freshly created, no created_at yet -> highest id wins via fallback
			e(2, '2026-07-01T10:00:00Z'),
		]);
		expect(sorted.map(x => x.task_id)).toEqual([99, 2, 1]);
	});

	it('survives undefined task_id without NaN comparisons', () => {
		const sorted = sortUnscheduledNewestFirst([
			e(undefined),
			e(3),
			e(undefined),
			e(7),
		]);
		expect(sorted.map(x => x.task_id)).toEqual([7, 3, undefined, undefined]);
	});

	it('does not mutate the input array', () => {
		const input = [e(1, '2026-01-01T10:00:00Z'), e(2, '2026-02-01T10:00:00Z')];
		const copy = [...input];
		sortUnscheduledNewestFirst(input);
		expect(input).toEqual(copy);
	});
});
