import { galleryLayers } from '@/utils/galleryLayers';

const thumbs = { 1: 'thumb-1', 2: 'thumb-2' };
const fulls = { 1: 'full-1' };

describe('galleryLayers', () => {
	it('shows the thumbnail the list already has while the full image loads', () => {
		expect(galleryLayers(1, thumbs, fulls, {})).toEqual({
			thumb: 'thumb-1',
			full: 'full-1',
			showThumb: true,
		});
	});

	it('drops the thumbnail once the full image has painted', () => {
		expect(galleryLayers(1, thumbs, fulls, { 1: true })).toEqual({
			thumb: 'thumb-1',
			full: 'full-1',
			showThumb: false,
		});
	});

	it('keeps showing the thumbnail when there is no full image yet', () => {
		expect(galleryLayers(2, thumbs, fulls, {})).toEqual({
			thumb: 'thumb-2',
			full: null,
			showThumb: true,
		});
	});

	it('has nothing to draw for an image the list never resolved', () => {
		expect(galleryLayers(3, thumbs, fulls, {})).toEqual({
			thumb: null,
			full: null,
			showThumb: false,
		});
	});

	it('has nothing to draw without an image at all', () => {
		expect(galleryLayers(undefined, thumbs, fulls, {})).toEqual({
			thumb: null,
			full: null,
			showThumb: false,
		});
	});

	it('a loaded flag from another image does not hide this thumbnail', () => {
		expect(galleryLayers(2, thumbs, fulls, { 1: true }).showThumb).toBe(true);
	});
});
