import {
	clipboardImageName,
	imagesFromClipboard,
} from '@/utils/clipboardImages';

const file = (name: string, type: string) => ({ name, type } as File);

const item = (kind: string, type: string, f: File | null) =>
	({ kind, type, getAsFile: () => f } as unknown as DataTransferItem);

describe('imagesFromClipboard', () => {
	it('picks the screenshot out of a paste', () => {
		const png = file('image.png', 'image/png');

		expect(
			imagesFromClipboard([item('file', 'image/png', png)] as never),
		).toEqual([png]);
	});

	it('ignores a plain text paste', () => {
		expect(
			imagesFromClipboard([item('string', 'text/plain', null)] as never),
		).toEqual([]);
	});

	it('ignores a pasted file that is not an image', () => {
		expect(
			imagesFromClipboard([
				item('file', 'application/pdf', file('a.pdf', 'application/pdf')),
			] as never),
		).toEqual([]);
	});

	it('takes every image when several are pasted at once', () => {
		const a = file('a.png', 'image/png');
		const b = file('b.jpg', 'image/jpeg');

		expect(
			imagesFromClipboard([
				item('file', 'image/png', a),
				item('string', 'text/html', null),
				item('file', 'image/jpeg', b),
			] as never),
		).toEqual([a, b]);
	});

	it('survives a clipboard with nothing in it', () => {
		expect(imagesFromClipboard(null)).toEqual([]);
		expect(imagesFromClipboard([] as never)).toEqual([]);
	});

	it('drops an item that claims to be a file but yields none', () => {
		expect(
			imagesFromClipboard([item('file', 'image/png', null)] as never),
		).toEqual([]);
	});
});

describe('clipboardImageName', () => {
	const at = new Date('2026-09-13T14:05:09Z');

	it('names a screenshot after the moment it was pasted', () => {
		expect(clipboardImageName('image/png', at)).toBe(
			'screenshot-2026-09-13-140509.png',
		);
	});

	it('keeps the pasted type in the extension', () => {
		expect(clipboardImageName('image/jpeg', at)).toBe(
			'screenshot-2026-09-13-140509.jpg',
		);
		expect(clipboardImageName('image/webp', at)).toBe(
			'screenshot-2026-09-13-140509.webp',
		);
	});

	it('falls back to png for a type it does not know', () => {
		expect(clipboardImageName('image/heif-sequence', at)).toBe(
			'screenshot-2026-09-13-140509.png',
		);
	});
});
