import {
	attachmentErrorMessage,
	formatFileSize,
	isImageMime,
	preflightError,
} from '../attachments';

describe('formatFileSize', () => {
	it('renders sizes people recognise', () => {
		expect(formatFileSize(0)).toBe('0 B');
		expect(formatFileSize(512)).toBe('512 B');
		expect(formatFileSize(1024)).toBe('1 KB');
		expect(formatFileSize(1536)).toBe('1.5 KB');
		expect(formatFileSize(25 * 1024 * 1024)).toBe('25 MB');
	});

	it('survives a missing size', () => {
		expect(formatFileSize(null)).toBe('');
		expect(formatFileSize(undefined)).toBe('');
	});
});

describe('isImageMime', () => {
	it('recognises the types a browser renders', () => {
		expect(isImageMime('image/png')).toBe(true);
		expect(isImageMime('IMAGE/JPEG')).toBe(true);
	});

	it('rejects everything else, including svg', () => {
		// The API never serves svg inline, so the UI must not preview it either.
		expect(isImageMime('image/svg+xml')).toBe(false);
		expect(isImageMime('application/pdf')).toBe(false);
		expect(isImageMime(null)).toBe(false);
		expect(isImageMime(undefined)).toBe(false);
	});
});

describe('attachmentErrorMessage', () => {
	const httpError = (status: number, message?: string) => ({
		response: { status, data: message ? { message } : {} },
	});

	it('explains the size limit using the limit the API reports', () => {
		expect(attachmentErrorMessage(httpError(413), 26214400)).toBe(
			'File is too large. The limit is 25 MB.',
		);
	});

	it('falls back to a plain sentence when no limit is known', () => {
		expect(attachmentErrorMessage(httpError(413))).toBe('File is too large.');
	});

	it('explains an unsupported type', () => {
		expect(attachmentErrorMessage(httpError(415))).toBe(
			'This file type cannot be attached.',
		);
	});

	it('passes a validation message through', () => {
		expect(attachmentErrorMessage(httpError(400, 'File size is required'))).toBe(
			'File size is required',
		);
	});

	it('names a storage rejection for what it is', () => {
		expect(attachmentErrorMessage(new Error('storage responded 403'))).toBe(
			'Upload failed. Please try again.',
		);
	});
});

describe('preflightError', () => {
	const file = (size: number, type = 'image/png') => ({ size, type }) as File;

	it('stops an empty file before the API answers with a validation blob', () => {
		// The API rejects size 0 with "The given data was invalid.", which tells nobody anything.
		expect(preflightError(file(0))).toBe('File is empty.');
	});

	it('lets a real file through', () => {
		expect(preflightError(file(10))).toBeNull();
	});

	it('names a file the browser could not type', () => {
		expect(preflightError(file(10, ''))).toBe(
			'This file type cannot be attached.',
		);
	});
});
