import { galleryImages, stepIndex } from '../galleryNavigation';

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
	const file = (id: number, mime: string | null) =>
		({ id, name: `f${id}`, mime_type: mime }) as never;

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
