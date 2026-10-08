import { pickDropTarget } from '../useRoutineDrag';

const pick = (kind: string) => {
	const target: any = {
		dataset: { drDrop: '1', drKind: kind, drDate: '2026-10-08' },
	};
	target.closest = () => target;
	(global as any).document = { elementsFromPoint: () => [target] };
	return pickDropTarget(0, 0)?.payload;
};

describe('pickDropTarget', () => {
	it('marks the unscheduled zone as undated', () => {
		expect(pick('unscheduled')).toEqual({
			date: '2026-10-08',
			allDay: true,
			unscheduled: true,
		});
	});

	it('keeps the week all-day row dated', () => {
		expect(pick('all-day')).toEqual({ date: '2026-10-08', allDay: true });
	});
});
