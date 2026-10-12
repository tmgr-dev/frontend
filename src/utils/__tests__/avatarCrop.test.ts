import {
	clampOffset,
	clampZoom,
	coverScale,
	MAX_ZOOM,
	outputName,
	outputSize,
	outputType,
	sourceRect,
} from '@/utils/avatarCrop';

const VIEW = 256;

describe('coverScale', () => {
	it('fits the shorter side to the view', () => {
		expect(coverScale(1000, 500, VIEW)).toBeCloseTo(VIEW / 500);
		expect(coverScale(400, 800, VIEW)).toBeCloseTo(VIEW / 400);
		expect(coverScale(128, 128, VIEW)).toBeCloseTo(2);
	});
});

describe('clampZoom', () => {
	it('keeps zoom between cover and the maximum', () => {
		expect(clampZoom(0.5)).toBe(1);
		expect(clampZoom(2)).toBe(2);
		expect(clampZoom(10)).toBe(MAX_ZOOM);
	});
});

describe('clampOffset', () => {
	const scale = coverScale(1000, 500, VIEW);

	it('lets a landscape image pan sideways only up to its edges', () => {
		const limit = (1000 * scale - VIEW) / 2;
		expect(clampOffset({ x: 9999, y: 50 }, 1000, 500, scale, VIEW)).toEqual({
			x: limit,
			y: 0,
		});
		expect(clampOffset({ x: -9999, y: -50 }, 1000, 500, scale, VIEW)).toEqual({
			x: -limit,
			y: 0,
		});
	});

	it('leaves an offset inside the limits alone', () => {
		expect(clampOffset({ x: 10, y: 0 }, 1000, 500, scale, VIEW)).toEqual({
			x: 10,
			y: 0,
		});
	});

	it('allows vertical pan once zoomed in', () => {
		const zoomed = scale * 2;
		expect(clampOffset({ x: 0, y: 9999 }, 1000, 500, zoomed, VIEW).y).toBe(
			(500 * zoomed - VIEW) / 2,
		);
	});
});

describe('sourceRect', () => {
	it('takes the centred square at zoom 1', () => {
		const landscape = sourceRect(
			{ x: 0, y: 0 },
			1000,
			500,
			coverScale(1000, 500, VIEW),
			VIEW,
		);
		expect(landscape.sx).toBeCloseTo(250);
		expect(landscape.sy).toBeCloseTo(0);
		expect(landscape.size).toBeCloseTo(500);

		const portrait = sourceRect(
			{ x: 0, y: 0 },
			400,
			800,
			coverScale(400, 800, VIEW),
			VIEW,
		);
		expect(portrait.sx).toBeCloseTo(0);
		expect(portrait.sy).toBeCloseTo(200);
		expect(portrait.size).toBeCloseTo(400);
	});

	it('follows pan and zoom', () => {
		const scale = coverScale(1000, 500, VIEW) * 2;
		const offset = clampOffset({ x: 9999, y: 9999 }, 1000, 500, scale, VIEW);
		const rect = sourceRect(offset, 1000, 500, scale, VIEW);
		expect(rect.size).toBeCloseTo(250);
		expect(rect.sx).toBeCloseTo(0);
		expect(rect.sy).toBeCloseTo(0);

		const moved = sourceRect({ x: -64, y: 0 }, 1000, 500, scale, VIEW);
		expect(moved.sx).toBeCloseTo(375 + 64 / scale);
		expect(moved.sy).toBeCloseTo(125);
	});

	it('stays within the image even for an unclamped offset', () => {
		for (const [w, h] of [
			[1000, 500],
			[400, 800],
			[300, 300],
		]) {
			for (const zoom of [1, 2.5, MAX_ZOOM]) {
				const scale = coverScale(w, h, VIEW) * zoom;
				for (const offset of [
					{ x: 5000, y: -5000 },
					{ x: -5000, y: 5000 },
					{ x: 0, y: 0 },
				]) {
					const { sx, sy, size } = sourceRect(offset, w, h, scale, VIEW);
					expect(sx).toBeGreaterThanOrEqual(0);
					expect(sy).toBeGreaterThanOrEqual(0);
					expect(sx + size).toBeLessThanOrEqual(w + 1e-9);
					expect(sy + size).toBeLessThanOrEqual(h + 1e-9);
				}
			}
		}
	});
});

describe('output', () => {
	it('caps the size at 512 and never upscales', () => {
		expect(outputSize(2000)).toBe(512);
		expect(outputSize(300.4)).toBe(300);
		expect(outputSize(0.2)).toBe(1);
	});

	it('keeps JPEG as JPEG and sends anything else as PNG with a matching name', () => {
		expect(outputType('image/jpeg')).toBe('image/jpeg');
		expect(outputType('image/webp')).toBe('image/png');
		expect(outputType('image/gif')).toBe('image/png');
		expect(outputName('image/jpeg')).toBe('avatar.jpg');
		expect(outputName('image/png')).toBe('avatar.png');
	});
});
