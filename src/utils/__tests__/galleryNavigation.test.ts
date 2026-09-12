import { galleryImages, stepIndex, type GalleryImage } from '../galleryNavigation';

describe('stepIndex', () => {
	it('walks forward and backward', () => {
		expect(stepIndex(0, 3, 1)).toBe(1);
		expect(stepIndex(2, 3, -1)).toBe(1);
	});

	it('wraps at both ends, so arrows never dead-end', () => {
		expect(stepIndex(2, 3, 1)).toBe(0);
		expect(stepIndex(0, 3, -1)).toBe(2);
	});

	it('stays put when there is nothing to walk', () => {
		expect(stepIndex(0, 1, 1)).toBe(0);
		expect(stepIndex(0, 0, 1)).toBe(0);
		expect(stepIndex(-1, 3, 1)).toBe(0);
	});
});

describe('galleryImages', () => {
	const file = (id: number, mime: string | null): GalleryImage => ({
		id,
		name: `f${id}`,
		size: 10,
		mime_type: mime,
	});

	it('keeps whatever the caller passed in, not a narrowed copy', () => {
		// The workspace file list carries a task on every row; the gallery must not strip it.
		const row = { id: 1, name: 'a.png', size: 10, mime_type: 'image/png', task: { id: 7 } };

		expect(galleryImages([row])[0].task.id).toBe(7);
	});

	it('keeps only what the gallery can show, in list order', () => {
		expect(
			galleryImages([
				file(1, 'image/png'),
				file(2, 'application/pdf'),
				file(3, 'image/jpeg'),
				file(4, 'image/svg+xml'),
				file(5, null),
			]).map((f) => f.id),
		).toEqual([1, 3]);
	});
});
