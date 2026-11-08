import { readdirSync, readFileSync } from 'node:fs';
import { join } from 'node:path';
import type { LocaleCode } from './locale';

type Tree = Record<string, unknown>;

const root = join(__dirname, '..', 'locales');

const read = (locale: string): Tree => {
	let files: string[] = [];
	try {
		files = readdirSync(join(root, locale));
	} catch {
		return {};
	}
	return Object.fromEntries(
		files
			.filter((file) => file.endsWith('.json'))
			.map((file) => [
				file.replace(/\.json$/, ''),
				JSON.parse(readFileSync(join(root, locale, file), 'utf8')),
			]),
	);
};

export const englishMessages: Tree = read('en');

export const loadMessages = async (locale: LocaleCode): Promise<Tree> =>
	read(locale);
