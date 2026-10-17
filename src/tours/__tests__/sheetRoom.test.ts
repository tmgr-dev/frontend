/** @jest-environment jsdom */
import { createSheetRoom, isAnchorClear } from '../overlay/sheetRoom';

const rect = (top: number, height: number) => ({
	top,
	bottom: top + height,
	height,
	left: 0,
	right: 10,
	width: 10,
	x: 0,
	y: top,
	toJSON: () => ({}),
});

function anchorAt(top: number, height: number) {
	const el = document.createElement('div');
	el.getBoundingClientRect = () => rect(top, height) as DOMRect;
	el.scrollIntoView = jest.fn();
	document.body.appendChild(el);
	return el;
}

beforeEach(() => {
	document.body.innerHTML = '';
	Object.defineProperty(window, 'innerHeight', {
		value: 800,
		configurable: true,
	});
	jest.useFakeTimers();
});

afterEach(() => jest.useRealTimers());

describe('isAnchorClear', () => {
	it('requires the whole anchor inside the allowed height', () => {
		expect(isAnchorClear(anchorAt(100, 200), true)).toBe(true);
		expect(isAnchorClear(anchorAt(300, 200), true)).toBe(false);
		expect(isAnchorClear(anchorAt(-10, 100), false)).toBe(false);
	});

	it('treats an anchor taller than the allowed height as clear when its top is in view', () => {
		expect(isAnchorClear(anchorAt(50, 900), true)).toBe(true);
		expect(isAnchorClear(anchorAt(-50, 900), true)).toBe(false);
		expect(isAnchorClear(anchorAt(500, 900), true)).toBe(false);
	});
});

describe('createSheetRoom', () => {
	it('rescrolls an anchor that is not clear', () => {
		const anchor = anchorAt(600, 100);
		const room = createSheetRoom();
		room.watch(anchor, true);
		jest.advanceTimersByTime(400);
		expect(anchor.scrollIntoView).toHaveBeenCalledTimes(1);
		room.release();
	});

	it('does not scroll a tall anchor whose top is in view', () => {
		const anchor = anchorAt(40, 1200);
		const room = createSheetRoom();
		room.watch(anchor, true);
		jest.advanceTimersByTime(2000);
		expect(anchor.scrollIntoView).not.toHaveBeenCalled();
		room.release();
	});

	it.each(['wheel', 'touchstart', 'touchmove'])(
		'stops forced scrolls after %s input',
		(type) => {
			const anchor = anchorAt(600, 100);
			const room = createSheetRoom();
			room.watch(anchor, true);
			window.dispatchEvent(new Event(type));
			jest.advanceTimersByTime(2000);
			expect(anchor.scrollIntoView).not.toHaveBeenCalled();
			room.release();
		},
	);
});
