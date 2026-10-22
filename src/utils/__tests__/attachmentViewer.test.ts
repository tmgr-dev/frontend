import {
	actualSizeZoom,
	clampPan,
	clampZoom,
	previewKind,
	withinTextPreviewLimit,
	ZOOM_MAX,
	ZOOM_MIN,
	zoomByWheel,
	zoomIn,
	zoomOut,
} from '../attachmentViewer';

const DOCX =
	'application/vnd.openxmlformats-officedocument.wordprocessingml.document';

describe('previewKind', () => {
	it.each([
		['a.png', 'image/png', 'image'],
		['a.bin', 'image/webp', 'image'],
		['a.pdf', 'application/pdf', 'pdf'],
		['a.PDF', null, 'pdf'],
		['a.pdf', 'application/octet-stream', 'pdf'],
		['a.md', 'text/markdown', 'markdown'],
		['a.md', null, 'markdown'],
		['a.markdown', 'text/plain', 'markdown'],
		['a.txt', 'text/plain; charset=utf-8', 'text'],
		['a.log', null, 'text'],
		['a.ts', 'application/octet-stream', 'text'],
		['a.json', 'application/json', 'text'],
		['a.bin', 'text/x-custom', 'text'],
		['a.docx', DOCX, 'other'],
		['a.svg', 'image/svg+xml', 'other'],
		['noext', null, 'other'],
	])('%s (%s) is %s', (name, mime_type, expected) => {
		expect(previewKind({ name, mime_type })).toBe(expected);
	});
});

describe('withinTextPreviewLimit', () => {
	it('allows up to 1 MB and treats an unknown size as over the limit', () => {
		expect(withinTextPreviewLimit(1024 * 1024)).toBe(true);
		expect(withinTextPreviewLimit(null)).toBe(false);
		expect(withinTextPreviewLimit(undefined)).toBe(false);
		expect(withinTextPreviewLimit(1024 * 1024 + 1)).toBe(false);
	});
});

describe('zoom', () => {
	it('clamps to [fit, max]', () => {
		expect(clampZoom(0.2)).toBe(ZOOM_MIN);
		expect(clampZoom(99)).toBe(ZOOM_MAX);
		expect(clampZoom(NaN)).toBe(ZOOM_MIN);
	});

	it('steps in and out within bounds', () => {
		expect(zoomIn(1)).toBeGreaterThan(1);
		expect(zoomOut(1)).toBe(ZOOM_MIN);
		expect(zoomIn(ZOOM_MAX)).toBe(ZOOM_MAX);
		expect(zoomOut(zoomIn(2))).toBeCloseTo(2);
	});

	it('wheel up zooms in, down zooms out, pinch is finer than wheel', () => {
		expect(zoomByWheel(2, -100, false)).toBeGreaterThan(2);
		expect(zoomByWheel(2, 100, false)).toBeLessThan(2);
		expect(zoomByWheel(2, -10, true)).toBeGreaterThan(
			zoomByWheel(2, -10, false),
		);
		expect(zoomByWheel(7.9, -1000, true)).toBe(ZOOM_MAX);
	});

	it('actual size is natural over fitted width, never below fit', () => {
		expect(actualSizeZoom(2000, 500)).toBe(4);
		expect(actualSizeZoom(300, 500)).toBe(ZOOM_MIN);
		expect(actualSizeZoom(10000, 100)).toBe(ZOOM_MAX);
		expect(actualSizeZoom(500, 0)).toBe(ZOOM_MIN);
	});
});

describe('clampPan', () => {
	const view = { width: 800, height: 600 };
	const content = { width: 400, height: 300 };

	it('allows no pan while the image fits', () => {
		expect(clampPan({ x: 50, y: -50 }, 1, view, content)).toEqual({
			x: 0,
			y: 0,
		});
	});

	it('limits the pan so the image edge stops at the view edge', () => {
		expect(clampPan({ x: 9999, y: -9999 }, 4, view, content)).toEqual({
			x: 400,
			y: -300,
		});
		expect(clampPan({ x: 10, y: 20 }, 4, view, content)).toEqual({
			x: 10,
			y: 20,
		});
	});
});
