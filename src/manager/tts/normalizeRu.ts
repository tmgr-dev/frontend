const DIGIT = [
	'ноль',
	'один',
	'два',
	'три',
	'четыре',
	'пять',
	'шесть',
	'семь',
	'восемь',
	'девять',
];
const FEMININE_UNIT = ['', 'одна', 'две'];
const TEENS = [
	'десять',
	'одиннадцать',
	'двенадцать',
	'тринадцать',
	'четырнадцать',
	'пятнадцать',
	'шестнадцать',
	'семнадцать',
	'восемнадцать',
	'девятнадцать',
];
const TENS = [
	'',
	'',
	'двадцать',
	'тридцать',
	'сорок',
	'пятьдесят',
	'шестьдесят',
	'семьдесят',
	'восемьдесят',
	'девяносто',
];
const HUNDREDS = [
	'',
	'сто',
	'двести',
	'триста',
	'четыреста',
	'пятьсот',
	'шестьсот',
	'семьсот',
	'восемьсот',
	'девятьсот',
];
const SCALES: { forms: [string, string, string]; feminine?: boolean }[] = [
	{ forms: ['', '', ''] },
	{ forms: ['тысяча', 'тысячи', 'тысяч'], feminine: true },
	{ forms: ['миллион', 'миллиона', 'миллионов'] },
	{ forms: ['миллиард', 'миллиарда', 'миллиардов'] },
	{ forms: ['триллион', 'триллиона', 'триллионов'] },
];
const PERCENT: [string, string, string] = ['процент', 'процента', 'процентов'];
const ORDINALS: Record<number, string> = {
	1: 'первый',
	2: 'второй',
	3: 'третий',
	4: 'четвёртый',
	5: 'пятый',
	6: 'шестой',
	7: 'седьмой',
	8: 'восьмой',
	9: 'девятый',
	10: 'десятый',
	11: 'одиннадцатый',
	12: 'двенадцатый',
	13: 'тринадцатый',
	14: 'четырнадцатый',
	15: 'пятнадцатый',
	16: 'шестнадцатый',
	17: 'семнадцатый',
	18: 'восемнадцатый',
	19: 'девятнадцатый',
	20: 'двадцатый',
	30: 'тридцатый',
	40: 'сороковой',
	50: 'пятидесятый',
	60: 'шестидесятый',
	70: 'семидесятый',
	80: 'восьмидесятый',
	90: 'девяностый',
	100: 'сотый',
};
const MAX_DIGITS = 15;

const spell = (digits: string): string =>
	[...digits].map((d) => DIGIT[Number(d)]).join(' ');

const pluralForm = (
	n: number,
	[one, few, many]: [string, string, string],
): string => {
	const tail = n % 100;
	if (tail >= 11 && tail <= 14) return many;
	const last = n % 10;
	if (last === 1) return one;
	if (last >= 2 && last <= 4) return few;
	return many;
};

const unit = (d: number, feminine: boolean): string =>
	feminine && (d === 1 || d === 2) ? FEMININE_UNIT[d] : DIGIT[d];

const groupWords = (n: number, feminine: boolean): string[] => {
	const out: string[] = [];
	const hundreds = Math.floor(n / 100);
	const rest = n % 100;
	if (hundreds) out.push(HUNDREDS[hundreds]);
	if (rest >= 20) {
		out.push(TENS[Math.floor(rest / 10)]);
		if (rest % 10) out.push(unit(rest % 10, feminine));
	} else if (rest >= 10) {
		out.push(TEENS[rest - 10]);
	} else if (rest) {
		out.push(unit(rest, feminine));
	}
	return out;
};

const cardinal = (digits: string): string => {
	if (
		digits.length > MAX_DIGITS ||
		(digits.length > 1 && digits.startsWith('0'))
	) {
		return spell(digits);
	}
	const n = Number(digits);
	if (n === 0) return DIGIT[0];
	const groups: number[] = [];
	for (let r = n; r > 0; r = Math.floor(r / 1000)) groups.push(r % 1000);
	const out: string[] = [];
	for (let i = groups.length - 1; i >= 0; i--) {
		if (!groups[i]) continue;
		out.push(...groupWords(groups[i], SCALES[i].feminine ?? false));
		if (i) out.push(pluralForm(groups[i], SCALES[i].forms));
	}
	return out.join(' ');
};

const readNumber = (token: string): string => {
	const [whole, fraction] = token.split(/[.,]/);
	return fraction === undefined
		? cardinal(whole)
		: `${cardinal(whole)} запятая ${spell(fraction)}`;
};

type Case =
	| 'nomM'
	| 'nomF'
	| 'nomN'
	| 'accF'
	| 'genM'
	| 'datM'
	| 'prepM'
	| 'obliqueF'
	| 'instrPl'
	| 'genPl'
	| 'datPl'
	| 'nomPl';

const CASE_BY_SUFFIX: Record<string, Case> = {
	й: 'nomM',
	я: 'nomF',
	ая: 'nomF',
	е: 'nomN',
	ое: 'nomN',
	ю: 'accF',
	ую: 'accF',
	го: 'genM',
	ого: 'genM',
	му: 'datM',
	м: 'prepM',
	ом: 'prepM',
	ей: 'obliqueF',
	ой: 'obliqueF',
	ми: 'instrPl',
	х: 'genPl',
	ых: 'genPl',
	ым: 'datPl',
	ые: 'nomPl',
};
const HARD_ENDINGS: Record<Case, string> = {
	nomM: 'ый',
	nomF: 'ая',
	nomN: 'ое',
	accF: 'ую',
	genM: 'ого',
	datM: 'ому',
	prepM: 'ом',
	obliqueF: 'ой',
	instrPl: 'ыми',
	genPl: 'ых',
	datPl: 'ым',
	nomPl: 'ые',
};
const SOFT_ENDINGS: Record<Case, string> = {
	nomM: 'ий',
	nomF: 'ья',
	nomN: 'ье',
	accF: 'ью',
	genM: 'ьего',
	datM: 'ьему',
	prepM: 'ьем',
	obliqueF: 'ьей',
	instrPl: 'ьими',
	genPl: 'ьих',
	datPl: 'ьим',
	nomPl: 'ьи',
};
const ORDINAL_SUFFIXES = Object.keys(CASE_BY_SUFFIX)
	.sort((x, y) => y.length - x.length)
	.join('|');
const ORDINAL_PATTERN = new RegExp(
	`(?<![\\p{L}\\d])(\\d+)-(${ORDINAL_SUFFIXES})(?![\\p{L}\\d])`,
	'gu',
);

const ordinal = (digits: string, suffix: string): string => {
	const masc = ORDINALS[Number(digits)];
	if (!masc) return cardinal(digits);
	const kind = CASE_BY_SUFFIX[suffix];
	if (masc.endsWith('ий')) return masc.slice(0, -2) + SOFT_ENDINGS[kind];
	const stressed = masc.endsWith('ой');
	if (stressed && kind === 'nomM') return masc;
	return masc.slice(0, -2) + HARD_ENDINGS[kind];
};

const time = (hours: string, minutes: string): string => {
	const m = Number(minutes);
	const words = cardinal(String(m));
	return `${cardinal(String(Number(hours)))} ${m < 10 ? `ноль ${words}` : words}`;
};

export const normalizeRussian = (text: string): string =>
	text
		.replace(/№\s*/g, 'номер ')
		.replace(/ & /g, ' и ')
		.replace(/ \+ /g, ' плюс ')
		.replace(/ = /g, ' равно ')
		.replace(/(?<=\p{L})-(?=\d)/gu, ' ')
		.replace(/(?<=\p{L})(?=\d)/gu, ' ')
		.replace(/(?<![\p{L}\d])-(?=\d)/gu, 'минус ')
		.replace(ORDINAL_PATTERN, (_, d: string, s: string) => ordinal(d, s))
		.replace(/(?<![\d.,])[1-9]\d{0,2}(?:,\d{3})+(?!\d|,\d)/g, (m) =>
			m.replace(/,/g, ''),
		)
		.replace(/(?<![\d.,])[1-9]\d{0,2}(?:[ \u00a0\u202f]\d{3})+(?!\d)/g, (m) =>
			m.replace(/[ \u00a0\u202f]/g, ''),
		)
		.replace(
			/(?<![\d:])(\d{1,2}):(\d{2})(?![\d:])/g,
			(_, h: string, m: string) => time(h, m),
		)
		.replace(/(?<![\d.,])\d+(?:\.\d+){2,}(?!\d|\.\d)/g, (m) =>
			m
				.split('.')
				.map((part) => cardinal(part))
				.join(' точка '),
		)
		.replace(/(?<![\d.,])(\d+(?:[.,]\d+)?)\s*%/g, (_, token: string) => {
			const [whole, fraction] = token.split(/[.,]/);
			const noun =
				fraction === undefined
					? pluralForm(Number(whole), PERCENT)
					: 'процента';
			return `${readNumber(token)} ${noun}`;
		})
		.replace(/(?<![\d.,])\d+[.,]\d+(?!\d|[.,]\d)/g, readNumber)
		.replace(/\d+/g, (m) => cardinal(m))
		.replace(/[ \t]{2,}/g, ' ')
		.trim();
