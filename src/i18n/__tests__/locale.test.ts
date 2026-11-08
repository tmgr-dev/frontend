import { detectLocale, normalizeLocale, pluralIndex } from '../locale';

describe('normalizeLocale', () => {
	it.each([
		['en', 'en'],
		['en-US', 'en'],
		['ru-RU', 'ru'],
		['pt', 'pt-BR'],
		['pt-PT', 'pt-BR'],
		['pt_BR', 'pt-BR'],
		['zh', 'zh-CN'],
		['zh-Hans-CN', 'zh-CN'],
		['zh-cn', 'zh-CN'],
		['de-AT', 'de'],
		['pl', 'pl'],
		['ja-JP', 'ja'],
		['ko-KR', 'ko'],
	])('%s -> %s', (tag, expected) => {
		expect(normalizeLocale(tag)).toBe(expected);
	});

	it.each([['zh-TW'], ['zh-Hant'], ['ar'], [''], [null], [42]])(
		'rejects %s',
		(tag) => {
			expect(normalizeLocale(tag)).toBeNull();
		},
	);
});

describe('detectLocale', () => {
	it('prefers the user setting, then storage, then the browser', () => {
		expect(detectLocale({ setting: 'de', stored: 'ru', browser: ['fr'] })).toBe(
			'de',
		);
		expect(detectLocale({ setting: null, stored: 'ru', browser: ['fr'] })).toBe(
			'ru',
		);
		expect(detectLocale({ browser: ['ar-EG', 'pl-PL'] })).toBe('pl');
	});

	it('falls back to English', () => {
		expect(detectLocale({ browser: ['ar', 'he'] })).toBe('en');
		expect(detectLocale({})).toBe('en');
	});
});

describe('pluralIndex', () => {
	it('uses one/few/many for Russian and Polish', () => {
		expect(
			[1, 2, 5, 11, 21, 22, 25].map((n) => pluralIndex('ru', n, 3)),
		).toEqual([0, 1, 2, 2, 0, 1, 2]);
		expect([1, 2, 5, 12, 22, 25].map((n) => pluralIndex('pl', n, 3))).toEqual([
			0, 1, 2, 2, 1, 2,
		]);
	});

	it('uses one/other for English-like languages', () => {
		expect([0, 1, 2, 1000000].map((n) => pluralIndex('en', n, 2))).toEqual([
			1, 0, 1, 1,
		]);
		expect([0, 1, 2, 1000000].map((n) => pluralIndex('fr', n, 2))).toEqual([
			0, 0, 1, 1,
		]);
		expect([1, 1000000].map((n) => pluralIndex('es', n, 2))).toEqual([0, 1]);
	});

	it('has a single form for CJK', () => {
		expect([1, 2, 10].map((n) => pluralIndex('ja', n, 1))).toEqual([0, 0, 0]);
	});

	it('reads an English fallback message with English rules', () => {
		expect([1, 5].map((n) => pluralIndex('ru', n, 2))).toEqual([0, 1]);
		expect([1, 5].map((n) => pluralIndex('ja', n, 2))).toEqual([0, 1]);
	});
});
