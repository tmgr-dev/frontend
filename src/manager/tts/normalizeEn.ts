const ONES = [
	'zero',
	'one',
	'two',
	'three',
	'four',
	'five',
	'six',
	'seven',
	'eight',
	'nine',
	'ten',
	'eleven',
	'twelve',
	'thirteen',
	'fourteen',
	'fifteen',
	'sixteen',
	'seventeen',
	'eighteen',
	'nineteen',
];
const TENS = [
	'',
	'',
	'twenty',
	'thirty',
	'forty',
	'fifty',
	'sixty',
	'seventy',
	'eighty',
	'ninety',
];
const SCALES = ['', 'thousand', 'million', 'billion', 'trillion'];
const ORDINAL_END: Record<string, string> = {
	one: 'first',
	two: 'second',
	three: 'third',
	five: 'fifth',
	eight: 'eighth',
	nine: 'ninth',
	twelve: 'twelfth',
};
const MAX_DIGITS = 15;

const below1000 = (n: number): string[] => {
	const out: string[] = [];
	if (n >= 100) {
		out.push(ONES[Math.floor(n / 100)], 'hundred');
		n %= 100;
	}
	if (n >= 20) {
		out.push(TENS[Math.floor(n / 10)]);
		n %= 10;
		if (n) out.push(ONES[n]);
	} else if (n || !out.length) {
		out.push(ONES[n]);
	}
	return out;
};

const cardinalWords = (digits: string): string[] => {
	const n = BigInt(digits);
	if (n === 0n) return ['zero'];
	const groups: number[] = [];
	for (let r = n; r > 0n; r /= 1000n) groups.push(Number(r % 1000n));
	const out: string[] = [];
	for (let i = groups.length - 1; i >= 0; i--) {
		if (!groups[i]) continue;
		out.push(...below1000(groups[i]));
		if (SCALES[i]) out.push(SCALES[i]);
	}
	return out;
};

const ordinalWords = (digits: string): string => {
	const words = cardinalWords(digits);
	const last = words[words.length - 1];
	words[words.length - 1] =
		ORDINAL_END[last] ??
		(last.endsWith('y') ? `${last.slice(0, -1)}ieth` : `${last}th`);
	return words.join(' ');
};

const spell = (digits: string): string =>
	[...digits].map((d) => ONES[Number(d)]).join(' ');

const cardinal = (digits: string): string => {
	const trimmed = digits.replace(/^0+(?=\d)/, '');
	return trimmed.length > MAX_DIGITS ||
		(digits.length > 1 && digits.startsWith('0'))
		? spell(digits)
		: cardinalWords(trimmed).join(' ');
};

const year = (digits: string): string => {
	const n = Number(digits);
	if (n % 1000 === 0 || (n >= 2001 && n <= 2009)) return cardinal(digits);
	const hi = digits.slice(0, 2);
	const lo = digits.slice(2);
	if (lo === '00') return `${cardinal(hi)} hundred`;
	return `${cardinal(hi)} ${lo.startsWith('0') ? `oh ${ONES[Number(lo)]}` : cardinal(lo)}`;
};

export const normalizeEnglish = (text: string): string =>
	text
		.replace(/(?<=\w)-(?=\w)/g, ' ')
		.replace(/(?<=[A-Za-z])(?=\d)/g, ' ')
		.replace(/(?<![\w.])-(?=\d)/g, 'minus ')
		.replace(/(\d+)(?:st|nd|rd|th)\b/gi, (_, d: string) => ordinalWords(d))
		.replace(
			/\b(\d{1,3}(?:,\d{3})+|\d+)\.(\d+)\b/g,
			(_, i: string, f: string) =>
				`${cardinal(i.replace(/,/g, ''))} point ${spell(f)}`,
		)
		.replace(/\b(\d{1,3}(?:,\d{3})+|\d+)\b/g, (m: string) => {
			const d = m.replace(/,/g, '');
			return m.length === 4 && Number(d) >= 1100 && Number(d) <= 2099
				? year(d)
				: cardinal(d);
		})
		.replace(/%/g, ' percent')
		.replace(/\s*&\s*/g, ' and ')
		.replace(/\s\+\s/g, ' plus ')
		.replace(/\s=\s/g, ' equals ')
		.replace(/[ \t]{2,}/g, ' ')
		.trim();
