import { clampOffset } from '../overlay/clamp';

const vp = { width: 1000, height: 800 };
const rect = (left: number, top: number, width: number, height: number) => ({
	left,
	top,
	right: left + width,
	bottom: top + height,
	width,
	height,
});

describe('clampOffset', () => {
	it('is zero when the rect already fits with padding', () => {
		expect(clampOffset(rect(100, 100, 300, 200), vp, 8)).toEqual({ dx: 0, dy: 0 });
	});

	it('pulls a rect hanging off the right and bottom back in', () => {
		expect(clampOffset(rect(800, 700, 400, 300), vp, 8)).toEqual({ dx: -208 - 0, dy: -208 + 0 });
	});

	it('pushes a rect off the left and top back in', () => {
		expect(clampOffset(rect(-50, -20, 300, 200), vp, 8)).toEqual({ dx: 58, dy: 28 });
	});

	it('aligns to the top-left when the rect is bigger than the viewport', () => {
		expect(clampOffset(rect(300, 300, 1200, 1000), vp, 8)).toEqual({ dx: -292, dy: -292 });
	});
});
