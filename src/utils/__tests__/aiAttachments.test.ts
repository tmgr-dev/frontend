import { setLocale } from '@/i18n';
import {
	MAX_ATTACHMENT_BYTES,
	attachmentErrorMessage,
	formatBytes,
	isSupportedAttachment,
	validateAttachment,
	withUniquePasteName,
} from '@/utils/aiAttachments';

describe('aiAttachments', () => {
	it('accepts documents, images and text by type or extension', () => {
		expect(
			isSupportedAttachment({ name: 'a.pdf', type: 'application/pdf' }),
		).toBe(true);
		expect(isSupportedAttachment({ name: 'shot', type: 'image/png' })).toBe(
			true,
		);
		expect(isSupportedAttachment({ name: 'spec.docx', type: '' })).toBe(true);
		expect(isSupportedAttachment({ name: 'notes', type: 'text/plain' })).toBe(
			true,
		);
		expect(
			isSupportedAttachment({ name: 'archive.zip', type: 'application/zip' }),
		).toBe(false);
		expect(
			isSupportedAttachment({ name: 'movie.mp4', type: 'video/mp4' }),
		).toBe(false);
	});

	it('rejects unsupported, oversized, empty and extra files', () => {
		expect(
			validateAttachment(
				{ name: 'a.zip', type: 'application/zip', size: 10 },
				0,
			),
		).toMatch(/not supported/);
		expect(
			validateAttachment(
				{
					name: 'a.pdf',
					type: 'application/pdf',
					size: MAX_ATTACHMENT_BYTES + 1,
				},
				0,
			),
		).toMatch(/larger than 10\.0 MB/);
		expect(
			validateAttachment(
				{ name: 'a.pdf', type: 'application/pdf', size: 0 },
				0,
			),
		).toMatch(/empty/);
		expect(
			validateAttachment(
				{ name: 'a.pdf', type: 'application/pdf', size: 5 },
				5,
			),
		).toMatch(/up to 5 files/);
		expect(
			validateAttachment(
				{ name: 'a.pdf', type: 'application/pdf', size: 5 },
				4,
			),
		).toBeNull();
	});

	it('maps backend error codes to composer messages', () => {
		const error = {
			response: { data: { error: 'too_many_pages', message: 'x' } },
		};
		expect(attachmentErrorMessage(error)).toMatch(/too many pages/);
		expect(
			attachmentErrorMessage({
				response: { data: { error: 'something_else' } },
			}),
		).toBeNull();
		expect(attachmentErrorMessage(new Error('boom'))).toBeNull();
	});

	it('renames pasted screenshots so they do not collide', () => {
		const pasted = new File(['x'], 'image.png', { type: 'image/png' });
		const renamed = withUniquePasteName(pasted, new Date(2026, 9, 21, 7, 5, 9));
		expect(renamed.name).toMatch(/^screenshot-20261021-070509-[a-z0-9]+\.png$/);
		const named = new File(['x'], 'design.png', { type: 'image/png' });
		expect(withUniquePasteName(named)).toBe(named);
	});
});

describe('formatBytes', () => {
	afterEach(async () => {
		await setLocale('en');
	});

	it('formats byte units in English', () => {
		expect(
			[0, 512, 2048, 1572864, MAX_ATTACHMENT_BYTES].map((b) => formatBytes(b)),
		).toEqual(['0 B', '512 B', '2 KB', '1.5 MB', '10.0 MB']);
	});

	it('uses the decimal separator of the current locale', async () => {
		await setLocale('de');
		expect(formatBytes(1572864)).toBe('1,5 MB');
	});
});
