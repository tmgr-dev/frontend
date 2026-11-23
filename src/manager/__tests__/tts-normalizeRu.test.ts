import { normalizeRussian } from '../tts/normalizeRu';

describe('normalizeRussian', () => {
	it.each([
		['0', 'ноль'],
		['1', 'один'],
		['2', 'два'],
		['5', 'пять'],
		['11', 'одиннадцать'],
		['12', 'двенадцать'],
		['14', 'четырнадцать'],
		['21', 'двадцать один'],
		['22', 'двадцать два'],
		['100', 'сто'],
		['101', 'сто один'],
		['112', 'сто двенадцать'],
		['1000', 'одна тысяча'],
		['1001', 'одна тысяча один'],
		['2000', 'две тысячи'],
		['5000', 'пять тысяч'],
		['21000', 'двадцать одна тысяча'],
		['100000', 'сто тысяч'],
		['1000000', 'один миллион'],
		['2000000', 'два миллиона'],
		['5000000', 'пять миллионов'],
		['1000000000', 'один миллиард'],
		[
			'999999999999',
			'девятьсот девяносто девять миллиардов девятьсот девяносто девять миллионов девятьсот девяносто девять тысяч девятьсот девяносто девять',
		],
	])('reads %s as %s', (input, spoken) => {
		expect(normalizeRussian(input)).toBe(spoken);
	});

	it('spells numbers over 15 digits one by one', () => {
		expect(normalizeRussian('1234567890123456')).toBe(
			'один два три четыре пять шесть семь восемь девять ноль один два три четыре пять шесть',
		);
	});

	it.each([
		['1%', 'один процент'],
		['2%', 'два процента'],
		['5%', 'пять процентов'],
		['11%', 'одиннадцать процентов'],
		['21%', 'двадцать один процент'],
		['22%', 'двадцать два процента'],
		['25 %', 'двадцать пять процентов'],
		['50%', 'пятьдесят процентов'],
		['100%', 'сто процентов'],
	])('reads percent %s as %s', (input, spoken) => {
		expect(normalizeRussian(input)).toBe(spoken);
	});

	it('reads ordinals by suffix gender', () => {
		expect(normalizeRussian('1-й, 2-я, 3-е и 5-го')).toBe(
			'первый, вторая, третье и пятый',
		);
	});

	it.each([
		['10-й', 'десятый'],
		['40-я', 'сороковая'],
		['100-е', 'сотое'],
		['3-му', 'третий'],
		['21-й', 'двадцать один'],
	])('reads ordinal %s as %s', (input, spoken) => {
		expect(normalizeRussian(input)).toBe(spoken);
	});

	it.each([
		['3,14', 'три запятая один четыре'],
		['3.14', 'три запятая один четыре'],
		['0,5', 'ноль запятая пять'],
		['-2,5', 'минус два запятая пять'],
	])('reads decimal %s as %s', (input, spoken) => {
		expect(normalizeRussian(input)).toBe(spoken);
	});

	it.each([
		['10:30', 'десять тридцать'],
		['09:05', 'девять ноль пять'],
		['23:59', 'двадцать три пятьдесят девять'],
	])('reads time %s as %s', (input, spoken) => {
		expect(normalizeRussian(input)).toBe(spoken);
	});

	it.each([
		['0.9.196', 'ноль точка девять точка сто девяносто шесть'],
		['1.2.3', 'один точка два точка три'],
	])('reads version %s as %s', (input, spoken) => {
		expect(normalizeRussian(input)).toBe(spoken);
	});

	it('reads ticket ids and glued Latin numbers', () => {
		expect(normalizeRussian('TM-1047')).toBe('TM одна тысяча сорок семь');
		expect(normalizeRussian('v2')).toBe('v два');
	});

	it.each([
		['-5', 'минус пять'],
		['-5 & 3 + 4 = 7', 'минус пять и три плюс четыре равно семь'],
		['2 + 2 = 4', 'два плюс два равно четыре'],
		['A & B', 'A и B'],
		['№7', 'номер семь'],
		['№ 7', 'номер семь'],
	])('reads symbols in %s as %s', (input, spoken) => {
		expect(normalizeRussian(input)).toBe(spoken);
	});

	it('leaves words after numbers untouched', () => {
		expect(
			normalizeRussian('Сессия 7 завершилась за 2 минуты, готово на 100%.'),
		).toBe('Сессия семь завершилась за два минуты, готово на сто процентов.');
	});

	it.each([['Привет, мир!'], ['Hello, world!'], ['кто-то там']])(
		'keeps text without digits or symbols unchanged: %s',
		(input) => {
			expect(normalizeRussian(input)).toBe(input);
		},
	);

	it('collapses repeated spaces and trims', () => {
		expect(normalizeRussian('  2   мира  ')).toBe('два мира');
	});
});
