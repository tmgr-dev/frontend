import { composeTitle } from '@/utils/categorySettingsPage';
import { titlePatternHandler } from '@/utils/titlePatternHandler';

const escapeRegex = (text: string) =>
	text.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');

const MASK_TOKENS: Record<string, string> = {
	yyyy: '\\d{4}',
	yy: '\\d{2}',
	mm: '\\d{2}',
	dd: '\\d{2}',
	hh: '\\d{2}',
	ii: '\\d{2}',
	ss: '\\d{2}',
	h: '\\d{1,2}',
	i: '\\d{1,2}',
	s: '\\d{1,2}',
	ddd: '(?:Mon|Tue|Wed|Thu|Fri|Sat|Sun)',
	dddd: '(?:Monday|Tuesday|Wednesday|Thursday|Friday|Saturday|Sunday)',
};

/** The backend matches a date mask with ".+?", which swallows any title and so never prefixes a date-only pattern; here each token matches its own shape. */
const maskRegex = (mask: string): string =>
	mask
		.toLowerCase()
		.replace(/yyyy|dddd|ddd|yy|mm|dd|hh|ii|ss|h|i|s|[a-z]+|[^a-z]+/g, (part) =>
			part in MASK_TOKENS
				? MASK_TOKENS[part]
				: /^[a-z]/.test(part)
				? '.+?'
				: escapeRegex(part),
		);

/** Port of the backend TaskTitlePattern.asRegex: indexes are digits, date masks match their rendered shape, the rest is literal. */
const patternRegex = (pattern: string): RegExp => {
	let source = '^';
	let last = 0;
	for (const match of pattern.matchAll(/\{index#[^}]+\}|\{dts#.*?#dte\}/g)) {
		const start = match.index ?? 0;
		source += escapeRegex(pattern.slice(last, start));
		source += match[0].startsWith('{index#')
			? '\\d+'
			: maskRegex(match[0].slice('{dts#'.length, -'#dte}'.length));
		last = start + match[0].length;
	}
	source += escapeRegex(pattern.slice(last)) + '(?:\\s*:|\\s|$)';
	return new RegExp(source);
};

export const hasIndexToken = (pattern: string): boolean =>
	pattern.includes('{index#');

export const alreadyPrefixed = (pattern: string, title: string): boolean =>
	Boolean(pattern) && patternRegex(pattern).test(title.trim());

export const renderPrefix = (
	pattern: string,
	indexes: Record<string, number> = {},
): string => titlePatternHandler(pattern, new Map(Object.entries(indexes)));

/** The muted text shown before the title input, e.g. `TM-158:`. */
export const prefixPreview = (prefix: string): string =>
	composeTitle(prefix, '').trim();

/** Same join as the backend TaskTitlePattern.compose: a title that already carries the pattern is left alone. */
export const composePatternTitle = (
	pattern: string,
	prefix: string,
	title: string,
): string => {
	const text = title.trim();
	if (!pattern || !text) return text || prefix;
	return alreadyPrefixed(pattern, text) ? text : composeTitle(prefix, text);
};
