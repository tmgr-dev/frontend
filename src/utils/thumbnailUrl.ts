/**
 * Asks a file content URL for the small rendering. Used on signed links, whose signature covers
 * the file, the user and the deadline rather than the whole query — so the same link may ask for
 * a thumbnail. A blob URL is already the bytes and is returned untouched.
 */
export const withThumbParam = (url: string): string => {
	if (url.startsWith('blob:') || url.includes('thumb=true')) {
		return url;
	}

	return `${url}${url.includes('?') ? '&' : '?'}thumb=true`;
};
