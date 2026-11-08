import { baseCompile } from '@intlify/message-compiler';
import { readdirSync, readFileSync, statSync } from 'node:fs';
import { join, relative } from 'node:path';
import { PLURAL_FORMS, SUPPORTED_LOCALES, type LocaleCode } from '../locale';

const src = join(__dirname, '..', '..');
const localesDir = join(src, 'locales');

type Flat = Record<string, string>;

const flatten = (value: unknown, prefix: string, out: Flat, file: string) => {
	if (typeof value === 'string') {
		out[prefix] = value;
		return;
	}
	if (value && typeof value === 'object' && !Array.isArray(value)) {
		for (const [key, child] of Object.entries(value)) {
			flatten(child, prefix ? `${prefix}.${key}` : key, out, file);
		}
		return;
	}
	throw new Error(`${file}: ${prefix} must be a string or an object`);
};

const readLocale = (locale: string): Flat => {
	const dir = join(localesDir, locale);
	const out: Flat = {};
	let files: string[] = [];
	try {
		files = readdirSync(dir).filter((file) => file.endsWith('.json'));
	} catch {
		return out;
	}
	for (const file of files) {
		const area = file.replace(/\.json$/, '');
		const path = join(dir, file);
		flatten(JSON.parse(readFileSync(path, 'utf8')), area, out, path);
	}
	return out;
};

const stripLiterals = (message: string) => message.replace(/\{'[^']*'\}/g, '');

const placeholders = (message: string) =>
	[...stripLiterals(message).matchAll(/\{\s*([A-Za-z_]\w*)\s*\}/g)]
		.map((m) => m[1])
		.filter((name) => name !== 'n' && name !== 'count')
		.sort();

const segments = (message: string) => stripLiterals(message).split('|').length;

const compileErrors = (message: string): string[] => {
	const errors: string[] = [];
	baseCompile(message, {
		onError: (error) => errors.push(error.message),
		jit: true,
	});
	return errors;
};

const english = readLocale('en');

describe('locale catalogs', () => {
	it('has only supported locale folders', () => {
		const folders = readdirSync(localesDir).filter((name) =>
			statSync(join(localesDir, name)).isDirectory(),
		);
		expect(
			folders.filter((f) => !SUPPORTED_LOCALES.includes(f as any)),
		).toEqual([]);
	});

	it('has non-empty English messages with one/other plurals', () => {
		const problems = Object.entries(english)
			.filter(([, message]) => message.trim() === '' || segments(message) > 2)
			.map(([key]) => key);
		expect(problems).toEqual([]);
	});

	it.each(SUPPORTED_LOCALES.map((locale) => [locale]))(
		'%s messages compile and match English',
		(locale) => {
			const messages = readLocale(locale);
			const problems: string[] = [];
			for (const [key, message] of Object.entries(messages)) {
				const errors = compileErrors(message);
				if (errors.length) problems.push(`${key}: ${errors.join('; ')}`);
				if (locale === 'en') continue;
				const source = english[key];
				if (source === undefined) {
					problems.push(`${key}: not in en`);
					continue;
				}
				if (placeholders(source).join() !== placeholders(message).join()) {
					problems.push(
						`${key}: placeholders ${placeholders(message)} != ${placeholders(source)}`,
					);
				}
				const expected =
					segments(source) > 1 ? PLURAL_FORMS[locale as LocaleCode].length : 1;
				if (segments(message) !== expected) {
					problems.push(
						`${key}: ${segments(message)} plural forms, expected ${expected}`,
					);
				}
			}
			expect(problems).toEqual([]);
		},
	);

	it('every literal key used in the source exists in English', () => {
		const files: string[] = [];
		const walk = (dir: string) => {
			for (const name of readdirSync(dir)) {
				if (name === '__tests__' || name === 'locales') continue;
				const path = join(dir, name);
				if (statSync(path).isDirectory()) walk(path);
				else if (/\.(vue|ts|js)$/.test(name)) files.push(path);
			}
		};
		walk(src);
		const usage =
			/(?:\$t|\bt|\$tc|\btc|\$te|\bte)\(\s*['"`]([\w-]+(?:\.[\w-]+)+)['"`]|keypath="([\w-]+(?:\.[\w-]+)+)"/g;
		const missing: string[] = [];
		for (const file of files) {
			const text = readFileSync(file, 'utf8');
			for (const match of text.matchAll(usage)) {
				const key = match[1] ?? match[2];
				if (english[key] === undefined)
					missing.push(`${relative(src, file)}: ${key}`);
			}
		}
		expect(missing).toEqual([]);
	});
});
