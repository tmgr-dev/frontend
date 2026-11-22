import {
	anchoredScrollTop,
	arrivalsAfter,
	isNearBottom,
	newBelowCount,
	shouldLoadOlder,
} from '../scrollLogic';

describe('isNearBottom', () => {
	const at = (distance: number) => ({
		scrollHeight: 1000,
		clientHeight: 400,
		scrollTop: 600 - distance,
	});

	it('is true within the 48 px bottom threshold', () => {
		expect(isNearBottom(at(0))).toBe(true);
		expect(isNearBottom(at(48))).toBe(true);
	});

	it('is false once scrolled further up than the threshold', () => {
		expect(isNearBottom(at(49))).toBe(false);
	});

	it('is true when the content does not overflow', () => {
		expect(
			isNearBottom({ scrollHeight: 300, clientHeight: 400, scrollTop: 0 }),
		).toBe(true);
	});
});

describe('shouldLoadOlder', () => {
	it('requests history only near the top and when idle', () => {
		expect(shouldLoadOlder(0, false, false)).toBe(true);
		expect(shouldLoadOlder(39, false, false)).toBe(true);
		expect(shouldLoadOlder(40, false, false)).toBe(false);
	});

	it('does not request while busy or after history is exhausted', () => {
		expect(shouldLoadOlder(0, true, false)).toBe(false);
		expect(shouldLoadOlder(0, false, true)).toBe(false);
	});
});

describe('arrivalsAfter', () => {
	const list = [{ id: 'a' }, { id: 'b' }, { id: 'c' }];

	it('returns the messages after the last tracked id', () => {
		expect(arrivalsAfter('a', list)).toEqual([{ id: 'b' }, { id: 'c' }]);
		expect(arrivalsAfter('c', list)).toEqual([]);
	});

	it('treats the whole list as arrivals when nothing is tracked yet', () => {
		expect(arrivalsAfter(undefined, list)).toEqual(list);
	});
});

describe('newBelowCount', () => {
	it('resets to zero while pinned to the bottom', () => {
		expect(newBelowCount(3, 2, true)).toBe(0);
	});

	it('accumulates arrivals while scrolled up', () => {
		expect(newBelowCount(3, 2, false)).toBe(5);
		expect(newBelowCount(0, 0, false)).toBe(0);
	});
});

describe('anchoredScrollTop', () => {
	it('shifts the scroll position by the height added above the viewport', () => {
		expect(anchoredScrollTop(10, 1000, 1500)).toBe(510);
	});

	it('keeps the position when nothing was prepended', () => {
		expect(anchoredScrollTop(10, 1000, 1000)).toBe(10);
	});
});
