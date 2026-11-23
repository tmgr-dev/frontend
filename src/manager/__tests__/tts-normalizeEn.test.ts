import { normalizeEnglish } from '../tts/normalizeEn';

describe('normalizeEnglish', () => {
	it.each([
		['0', 'zero'],
		['7', 'seven'],
		['13', 'thirteen'],
		['21', 'twenty one'],
		['100', 'one hundred'],
		['101', 'one hundred one'],
		['7234', 'seven thousand two hundred thirty four'],
		['1,000,000', 'one million'],
		['2500000000', 'two billion five hundred million'],
	])('reads %s as %s', (input, spoken) => {
		expect(normalizeEnglish(input)).toBe(spoken);
	});

	it('reads ordinals', () => {
		expect(normalizeEnglish('the 1st, 2nd, 3rd and 21st and 100th')).toBe(
			'the first, second, third and twenty first and one hundredth',
		);
	});

	it('reads decimals and percent', () => {
		expect(normalizeEnglish('3.14 and 50%')).toBe(
			'three point one four and fifty percent',
		);
	});

	it('reads years as pairs', () => {
		expect(normalizeEnglish('in 1999 and 2026')).toBe(
			'in nineteen ninety nine and twenty twenty six',
		);
		expect(normalizeEnglish('in 2000')).toBe('in two thousand');
	});

	it('reads negative numbers and a few symbols', () => {
		expect(normalizeEnglish('-5 & 3 + 4 = 7')).toBe(
			'minus five and three plus four equals seven',
		);
	});

	it('keeps tokens with letters and digits readable', () => {
		expect(normalizeEnglish('v2 and TM-1047')).toBe(
			'v two and TM one thousand forty seven',
		);
	});

	it('leaves huge numbers digit by digit', () => {
		expect(normalizeEnglish('1234567890123456')).toBe(
			'one two three four five six seven eight nine zero one two three four five six',
		);
	});
});
