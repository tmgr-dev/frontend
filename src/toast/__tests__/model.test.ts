import { ageLabel, glyphOf, heightOf, isOk } from '../model';

describe('toast model', () => {
	it('labels the age of a toast', () => {
		expect(ageLabel(10_000, 10_000)).toBe('now');
		expect(ageLabel(10_000, 20_000)).toBe('now');
		expect(ageLabel(130_000, 10_000)).toBe('2m');
		expect(ageLabel(3 * 3_600_000 + 5_000, 0)).toBe('3h');
	});

	it('picks a glyph and the ok tint per kind', () => {
		expect(glyphOf('decision')).toBe('?');
		expect(glyphOf('limit')).toBe('%');
		expect(isOk('agent')).toBe(true);
		expect(isOk('idle')).toBe(false);
	});

	it('rounds the measured height up', () => {
		expect(heightOf(120.2)).toBe(121);
		expect(heightOf(-3)).toBe(0);
	});
});
