export const MAX_ZOOM = 4;
export const OUTPUT_SIZE = 512;

export interface Offset {
	x: number;
	y: number;
}

/** Scale at which the shorter side of the image exactly fills a square view of side `view`. */
export const coverScale = (width: number, height: number, view: number) =>
	view / Math.min(width, height);

export const clampZoom = (zoom: number) =>
	Math.min(MAX_ZOOM, Math.max(1, zoom));

const clampAxis = (
	value: number,
	size: number,
	scale: number,
	view: number,
) => {
	const limit = Math.max(0, (size * scale - view) / 2);

	return limit === 0 ? 0 : Math.min(limit, Math.max(-limit, value));
};

/** The offset is the image centre relative to the view centre, in view pixels. */
export const clampOffset = (
	offset: Offset,
	width: number,
	height: number,
	scale: number,
	view: number,
): Offset => ({
	x: clampAxis(offset.x, width, scale, view),
	y: clampAxis(offset.y, height, scale, view),
});

/** The square of the natural image the view shows, for `drawImage`. */
export const sourceRect = (
	offset: Offset,
	width: number,
	height: number,
	scale: number,
	view: number,
) => {
	const size = Math.min(view / scale, width, height);
	const clamp = (start: number, max: number) =>
		Math.min(max - size, Math.max(0, start));

	return {
		sx: clamp(width / 2 - offset.x / scale - size / 2, width),
		sy: clamp(height / 2 - offset.y / scale - size / 2, height),
		size,
	};
};

export const outputSize = (sourceSize: number) =>
	Math.max(1, Math.min(OUTPUT_SIZE, Math.round(sourceSize)));

/** JPEG keeps photos small; everything else may carry transparency, so it goes out as PNG. */
export const outputType = (sourceType: string) =>
	sourceType === 'image/jpeg' ? 'image/jpeg' : 'image/png';

export const outputName = (type: string) =>
	type === 'image/jpeg' ? 'avatar.jpg' : 'avatar.png';
