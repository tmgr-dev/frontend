import { generateUniqueCategoryCode, sanitizeCategoryCode } from '../categoryCode';

describe('sanitizeCategoryCode', () => {
	it('transliterates Cyrillic', () => {
		expect(sanitizeCategoryCode('Разработка')).toBe('RAZRABOTKA');
		expect(sanitizeCategoryCode('Привет')).toBe('PRIVET');
		expect(sanitizeCategoryCode('ёжик')).toBe('EZHIK');
		expect(sanitizeCategoryCode('Щёлк')).toBe('SCHELK');
	});

	it('replaces non-alphanumerics with hyphens', () => {
		expect(sanitizeCategoryCode('Hello World')).toBe('HELLO-WORLD');
		expect(sanitizeCategoryCode('Bug #1!!!')).toBe('BUG-1');
		expect(sanitizeCategoryCode('a__b  c')).toBe('A-B-C');
	});

	it('defaults to CAT when nothing usable remains', () => {
		expect(sanitizeCategoryCode(null)).toBe('CAT');
		expect(sanitizeCategoryCode('')).toBe('CAT');
		expect(sanitizeCategoryCode('!!!')).toBe('CAT');
		expect(sanitizeCategoryCode('ъь')).toBe('CAT');
	});

	it('uppercases the result', () => {
		expect(sanitizeCategoryCode('backend')).toBe('BACKEND');
	});
});

describe('generateUniqueCategoryCode', () => {
	it('returns the base when free', () => {
		expect(generateUniqueCategoryCode('DEV', [])).toBe('DEV');
		expect(generateUniqueCategoryCode('DEV', ['OTHER'])).toBe('DEV');
	});

	it('appends an incrementing suffix when the base is taken', () => {
		expect(generateUniqueCategoryCode('DEV', ['DEV'])).toBe('DEV-1');
		expect(generateUniqueCategoryCode('DEV', ['DEV', 'DEV-1'])).toBe('DEV-2');
		expect(generateUniqueCategoryCode('DEV', ['DEV', 'DEV-1', 'DEV-2'])).toBe('DEV-3');
	});

	it('is case-insensitive', () => {
		expect(generateUniqueCategoryCode('DEV', ['dev'])).toBe('DEV-1');
	});

	it('truncates the base to fit fifty characters', () => {
		const longBase = 'A'.repeat(60);
		const result = generateUniqueCategoryCode(longBase, ['A'.repeat(50)]);
		expect(result.length).toBeLessThanOrEqual(50);
		expect(result).toBe(`${'A'.repeat(48)}-1`);
	});

	it('defaults to CAT when the base is empty', () => {
		expect(generateUniqueCategoryCode(null, [])).toBe('CAT');
		expect(generateUniqueCategoryCode('', [])).toBe('CAT');
		expect(generateUniqueCategoryCode('', ['CAT'])).toBe('CAT-1');
	});
});
