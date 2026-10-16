export type ImageLayout = 'left-50' | 'right-50' | 'full-width' | null;

export interface ImageTitle {
	layout: ImageLayout;
	width: number | null;
	height: number | null;
	source: string | null;
	title: string;
}

const LAYOUTS = ['left-50', 'right-50', 'full-width'] as const;
const SOURCE = /^\s*source=(\S+)\s*/;
const SIZE = /(?:^|\s)=(\d+)?x(\d+)?$/;
const FILENAME =
	/^[^\n/\\]+\.(png|jpe?g|gif|webp|svg|bmp|avif|heic|heif|tiff?)$/i;

// Outline keeps an image's layout and size in the markdown title slot:
// ![caption](src "right-50 My title =300x200")
export const parseImageTitle = (raw: string | null | undefined): ImageTitle => {
	let rest = raw ?? '';
	let source: string | null = null;
	const sourceMatch = rest.match(SOURCE);
	if (sourceMatch) {
		source = sourceMatch[1];
		rest = rest.slice(sourceMatch[0].length);
	}
	let layout: ImageLayout = null;
	for (const candidate of LAYOUTS) {
		const word = new RegExp(`(^|\\s)${candidate}(?=\\s|$)`);
		if (word.test(rest)) {
			layout = candidate;
			rest = rest.replace(word, '$1');
			break;
		}
	}
	let width: number | null = null;
	let height: number | null = null;
	const size = rest.match(SIZE);
	if (size && (size[1] || size[2])) {
		width = size[1] ? Number(size[1]) : null;
		height = size[2] ? Number(size[2]) : null;
		rest = rest.slice(0, size.index);
	}
	return { layout, width, height, source, title: rest.trim() };
};

export const formatImageTitle = (parts: ImageTitle): string => {
	const words = [
		parts.source ? `source=${parts.source}` : '',
		parts.layout ?? '',
		parts.title.trim(),
	].filter(Boolean);
	const width = parts.width == null ? '' : String(Math.round(parts.width));
	const height = parts.height == null ? '' : String(Math.round(parts.height));
	const size = width || height ? ` =${width}x${height}` : '';
	return words.join(' ') + size;
};

export const isFilenameAlt = (alt: string): boolean => FILENAME.test(alt);

const PNG_SIGNATURE = [137, 80, 78, 71, 13, 10, 26, 10];
const PIXELS_PER_METER_AT_72_DPI = 2834.5;

const readU32 = (bytes: Uint8Array, at: number): number =>
	((bytes[at] << 24) |
		(bytes[at + 1] << 16) |
		(bytes[at + 2] << 8) |
		bytes[at + 3]) >>>
	0;

// Retina screenshots carry 144 dpi in pHYs; Outline halves their size so they
// show at the size they had on screen.
export const pngPixelRatio = (bytes: Uint8Array): number => {
	if (PNG_SIGNATURE.some((byte, i) => bytes[i] !== byte)) return 1;
	let at = 8;
	while (at + 8 <= bytes.length) {
		const length = readU32(bytes, at);
		const type = String.fromCharCode(...bytes.subarray(at + 4, at + 8));
		if (type === 'pHYs' && at + 17 <= bytes.length) {
			if (bytes[at + 16] !== 1) return 1;
			const ratio = Math.round(
				readU32(bytes, at + 8) / PIXELS_PER_METER_AT_72_DPI,
			);
			return ratio >= 1 ? ratio : 1;
		}
		if (type === 'IDAT' || type === 'IEND') return 1;
		at += 12 + length;
	}
	return 1;
};
