import { t } from '@/i18n';

/** Formatting and error wording shared by the attachment UI. */

const UNITS = ['b', 'kb', 'mb', 'gb'];

/** Types the API is willing to serve inline, so the only ones worth previewing. */
const PREVIEWABLE = [
	'image/png',
	'image/jpeg',
	'image/gif',
	'image/webp',
	'image/avif',
];

export const formatFileSize = (bytes?: number | null): string => {
	if (bytes === null || bytes === undefined) {
		return '';
	}
	if (bytes < 1024) {
		return t('tasks.helpers.fileSize.b', { n: bytes });
	}
	let value = bytes;
	let unit = 0;
	while (value >= 1024 && unit < UNITS.length - 1) {
		value /= 1024;
		unit += 1;
	}
	return t(`tasks.helpers.fileSize.${UNITS[unit]}`, {
		n: Math.round(value * 10) / 10,
	});
};

export const isImageMime = (mime?: string | null): boolean =>
	!!mime && PREVIEWABLE.includes(mime.split(';')[0].trim().toLowerCase());

const statusOf = (error: unknown): number | null => {
	const response = (error as { response?: { status?: number } })?.response;
	return typeof response?.status === 'number' ? response.status : null;
};

const messageOf = (error: unknown): string | null => {
	const data = (error as { response?: { data?: { message?: string } } })
		?.response?.data;
	return typeof data?.message === 'string' && data.message
		? data.message
		: null;
};

/**
 * Turns whatever the upload threw into one sentence. The API answers 413 for size and 415 for type;
 * a failed PUT to storage is an opaque signature rejection with nothing worth quoting.
 */
export const attachmentErrorMessage = (
	error: unknown,
	maxBytes?: number | null,
): string => {
	switch (statusOf(error)) {
		case 413:
			return maxBytes
				? t('tasks.helpers.attachmentErrors.tooLargeWithLimit', {
						limit: formatFileSize(maxBytes),
					})
				: t('tasks.helpers.attachmentErrors.tooLarge');
		case 415:
			return t('tasks.helpers.attachmentErrors.typeNotAllowed');
		case 400:
			return (
				messageOf(error) ?? t('tasks.helpers.attachmentErrors.notAttachable')
			);
		case 403:
			return t('tasks.helpers.attachmentErrors.noAccess');
		default:
			return (
				messageOf(error) ?? t('tasks.helpers.attachmentErrors.uploadFailed')
			);
	}
};

/**
 * Catches the two cases whose server answer is unreadable: a zero-byte file trips the @Positive
 * validation and comes back as a generic "data was invalid", and a file the browser could not type
 * would be sent as application/octet-stream, which the allow list rejects on purpose.
 */
export const preflightError = (
	file: File,
	{ anyType = false }: { anyType?: boolean } = {},
): string | null => {
	if (!file.size) {
		return t('tasks.helpers.attachmentErrors.empty');
	}
	if (!file.type && !anyType) {
		return t('tasks.helpers.attachmentErrors.typeNotAllowed');
	}
	return null;
};
