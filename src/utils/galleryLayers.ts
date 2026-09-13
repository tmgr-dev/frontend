export interface GalleryLayers {
	/** The small rendering the list already resolved, drawn until the full image paints. */
	thumb: string | null;
	full: string | null;
	showThumb: boolean;
}

/**
 * TM-237 — the viewer stacks two images: the list's thumbnail underneath, the full image on top.
 * The thumbnail stays until the full image fires `load`, so opening the gallery draws something
 * at once instead of a spinner, and a full image that never arrives leaves the thumbnail in place.
 */
export function galleryLayers(
	fileId: number | undefined,
	thumbs: Record<number, string>,
	fulls: Record<number, string>,
	loaded: Record<number, boolean>,
): GalleryLayers {
	if (!fileId) {
		return { thumb: null, full: null, showThumb: false };
	}

	const thumb = thumbs[fileId] ?? null;
	const full = fulls[fileId] ?? null;

	return { thumb, full, showThumb: thumb !== null && !loaded[fileId] };
}
