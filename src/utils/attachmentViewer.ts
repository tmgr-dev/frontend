import { isImageMime } from '@/utils/attachments';

export type PreviewKind = 'image' | 'pdf' | 'markdown' | 'text' | 'other';

export const MAX_TEXT_PREVIEW_BYTES = 1024 * 1024;

export const ZOOM_MIN = 1;
export const ZOOM_MAX = 8;
export const ZOOM_STEP = 1.25;

const TEXT_MIME = [
	'application/json',
	'application/xml',
	'application/x-yaml',
	'application/yaml',
	'application/javascript',
	'application/x-sh',
	'application/sql',
	'application/toml',
];

const TEXT_EXTENSIONS = [
	'txt',
	'text',
	'log',
	'csv',
	'tsv',
	'json',
	'java',
	'ts',
	'tsx',
	'js',
	'jsx',
	'mjs',
	'py',
	'go',
	'rb',
	'php',
	'kt',
	'sql',
	'yaml',
	'yml',
	'xml',
	'html',
	'css',
	'sh',
	'vue',
	'rs',
	'c',
	'h',
	'cpp',
	'cs',
	'swift',
	'toml',
	'ini',
	'properties',
	'gradle',
];

const baseType = (mime: string | null | undefined): string =>
	(mime || '').split(';', 1)[0].trim().toLowerCase();

const extensionOf = (name: string): string => {
	const dot = name.lastIndexOf('.');
	return dot < 0 ? '' : name.slice(dot + 1).toLowerCase();
};

/** What the viewer can draw for a file. The stored mime may be null or generic, so the extension is consulted too. */
export const previewKind = (file: {
	name: string;
	mime_type?: string | null;
}): PreviewKind => {
	const mime = baseType(file.mime_type);
	const ext = extensionOf(file.name);
	if (isImageMime(mime)) return 'image';
	if (mime === 'application/pdf' || ext === 'pdf') return 'pdf';
	if (mime === 'text/markdown' || ext === 'md' || ext === 'markdown') {
		return 'markdown';
	}
	if (
		mime.startsWith('text/') ||
		TEXT_MIME.includes(mime) ||
		TEXT_EXTENSIONS.includes(ext)
	) {
		return 'text';
	}
	return 'other';
};

/** Text-like previews are read fully into memory, so they stop at a size cap; an unknown size can't be trusted to fit. */
export const withinTextPreviewLimit = (size: number | null | undefined) =>
	size != null && size <= MAX_TEXT_PREVIEW_BYTES;

export const clampZoom = (zoom: number): number => {
	if (!Number.isFinite(zoom)) return ZOOM_MIN;
	return Math.min(ZOOM_MAX, Math.max(ZOOM_MIN, zoom));
};

export const zoomIn = (zoom: number): number => clampZoom(zoom * ZOOM_STEP);
export const zoomOut = (zoom: number): number => clampZoom(zoom / ZOOM_STEP);

/** Plain wheel steps coarsely; a trackpad pinch arrives as ctrl+wheel with small deltas. */
export const zoomByWheel = (
	zoom: number,
	deltaY: number,
	ctrlKey: boolean,
): number => clampZoom(zoom * Math.exp(-deltaY * (ctrlKey ? 0.01 : 0.002)));

/** Zoom at which the image shows at its natural pixel size, given how wide fit currently draws it. */
export const actualSizeZoom = (naturalWidth: number, fittedWidth: number) =>
	fittedWidth > 0 ? clampZoom(naturalWidth / fittedWidth) : ZOOM_MIN;

export interface Point {
	x: number;
	y: number;
}

export interface Size {
	width: number;
	height: number;
}

/** Keeps a zoomed image from being dragged off the stage: the edge of the image stops at the edge of the view. */
export const clampPan = (
	pan: Point,
	zoom: number,
	view: Size,
	content: Size,
): Point => {
	const limit = (viewSide: number, contentSide: number) =>
		Math.max(0, (contentSide * zoom - viewSide) / 2);
	const maxX = limit(view.width, content.width);
	const maxY = limit(view.height, content.height);
	return {
		x: Math.min(maxX, Math.max(-maxX, pan.x)) || 0,
		y: Math.min(maxY, Math.max(-maxY, pan.y)) || 0,
	};
};
