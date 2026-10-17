import { collides, pickAvoidingPosition } from '../overlay/avoid';

const rect = (left: number, top: number, width: number, height: number) => ({
	left,
	top,
	right: left + width,
	bottom: top + height,
	width,
	height,
});

const vp = { width: 1000, height: 800 };
const card = rect(100, 300, 300, 200);

describe('pickAvoidingPosition', () => {
	it('returns null when the card does not touch an avoid rect', () => {
		expect(
			pickAvoidingPosition({
				card,
				anchor: null,
				avoid: [rect(600, 600, 100, 40)],
				viewport: vp,
			}),
		).toBeNull();
	});

	it('returns null when there is nothing to avoid', () => {
		expect(
			pickAvoidingPosition({ card, anchor: null, avoid: [], viewport: vp }),
		).toBeNull();
	});

	it('moves the card to a spot that clears the avoid rect and stays in the viewport', () => {
		const cta = rect(120, 400, 200, 44);
		const pos = pickAvoidingPosition({
			card,
			anchor: rect(120, 100, 300, 60),
			avoid: [cta],
			viewport: vp,
		})!;
		expect(pos).not.toBeNull();
		const moved = rect(pos.left, pos.top, card.width, card.height);
		expect(collides(moved, cta)).toBe(false);
		expect(moved.left).toBeGreaterThanOrEqual(8);
		expect(moved.top).toBeGreaterThanOrEqual(8);
		expect(moved.right).toBeLessThanOrEqual(992);
		expect(moved.bottom).toBeLessThanOrEqual(792);
	});

	it('prefers a spot that also clears the anchor', () => {
		const cta = rect(120, 400, 200, 44);
		const anchor = rect(120, 100, 300, 60);
		const pos = pickAvoidingPosition({
			card,
			anchor,
			avoid: [cta],
			viewport: vp,
		})!;
		expect(
			collides(rect(pos.left, pos.top, card.width, card.height), anchor),
		).toBe(false);
	});

	it('falls back to the least overlap when nothing fits', () => {
		const tall = rect(0, 0, 400, 780);
		const pos = pickAvoidingPosition({
			card: rect(300, 10, 400, 780),
			anchor: null,
			avoid: [rect(0, 0, 1000, 800)],
			viewport: vp,
		});
		expect(pos).not.toBeNull();
		expect(tall.width).toBe(400);
	});

	it('works with a tiny viewport by staying at the padded origin', () => {
		const pos = pickAvoidingPosition({
			card: rect(0, 0, 400, 400),
			anchor: null,
			avoid: [rect(10, 10, 50, 50)],
			viewport: { width: 360, height: 300 },
		})!;
		expect(pos.left).toBe(8);
		expect(pos.top).toBe(8);
	});
});

describe('collides', () => {
	it('treats touching edges as clear', () => {
		expect(collides(rect(0, 0, 10, 10), rect(10, 0, 10, 10))).toBe(false);
	});
	it('detects overlap', () => {
		expect(collides(rect(0, 0, 10, 10), rect(5, 5, 10, 10))).toBe(true);
	});
});
