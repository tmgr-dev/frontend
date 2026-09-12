/** Formatting and error wording shared by the attachment UI. */

const UNITS = ['B', 'KB', 'MB', 'GB'];

/** Types the API is willing to serve inline, so the only ones worth previewing. */
const PREVIEWABLE = ['image/png', 'image/jpeg', 'image/gif', 'image/webp', 'image/avif'];

export const formatFileSize = (bytes?: number | null): string => {
	if (bytes === null || bytes === undefined) {
		return '';
	}
	if (bytes < 1024) {
		return `${bytes} B`;
	}
	let value = bytes;
	let unit = 0;
	while (value >= 1024 && unit < UNITS.length - 1) {
		value /= 1024;
		unit += 1;
	}
	return `${Math.round(value * 10) / 10} ${UNITS[unit]}`;
};

export const isImageMime = (mime?: string | null): boolean =>
	!!mime && PREVIEWABLE.includes(mime.split(';')[0].trim().toLowerCase());

const statusOf = (error: unknown): number | null => {
	const response = (error as { response?: { status?: number } })?.response;
	return typeof response?.status === 'number' ? response.status : null;
};

const messageOf = (error: unknown): string | null => {
	const data = (error as { response?: { data?: { message?: string } } })?.response?.data;
	return typeof data?.message === 'string' && data.message ? data.message : null;
};

/**
 * Turns whatever the upload threw into one sentence. The API answers 413 for size and 415 for type;
 * a failed PUT to storage is an opaque signature rejection with nothing worth quoting.
 */
export const attachmentErrorMessage = (error: unknown, maxBytes?: number | null): string => {
	switch (statusOf(error)) {
		case 413:
			return maxBytes
				? `File is too large. The limit is ${formatFileSize(maxBytes)}.`
				: 'File is too large.';
		case 415:
			return 'This file type cannot be attached.';
		case 400:
			return messageOf(error) ?? 'This file cannot be attached.';
		case 403:
			return 'You do not have access to this file.';
		default:
			return messageOf(error) ?? 'Upload failed. Please try again.';
	}
};

/**
 * Catches the two cases whose server answer is unreadable: a zero-byte file trips the @Positive
 * validation and comes back as a generic "data was invalid", and a file the browser could not type
 * would be sent as application/octet-stream, which the allow list rejects on purpose.
 */
export const preflightError = (file: File): string | null => {
	if (!file.size) {
		return 'File is empty.';
	}
	if (!file.type) {
		return 'This file type cannot be attached.';
	}
	return null;
};
