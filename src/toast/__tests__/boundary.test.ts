import { readdirSync, readFileSync, statSync } from 'node:fs';
import { join } from 'node:path';

const TOAST_DIR = join(__dirname, '..');

const sourceFiles = (dir: string): string[] =>
	readdirSync(dir).flatMap((entry) => {
		const path = join(dir, entry);
		if (statSync(path).isDirectory()) {
			return entry === '__tests__' ? [] : sourceFiles(path);
		}
		return /\.(ts|vue)$/.test(entry) ? [path] : [];
	});

const importSpecifiers = (source: string): string[] => {
	const pattern =
		/(?:^|\n)\s*(?:import|export)\s[^'";]*?from\s+['"]([^'"]+)['"]|(?:^|\n)\s*import\s+['"]([^'"]+)['"]/g;
	return [...source.matchAll(pattern)].map((match) => match[1] ?? match[2]);
};

const ALLOWED = [
	/^\.{1,2}\//,
	/^vue$/,
	/^vue-i18n$/,
	/^@tauri-apps\/api(\/|$)/,
	/^@fontsource\//,
	/^@\/i18n(\/|$)/,
	/^@\/assets\//,
];

describe('toast import boundary', () => {
	const files = sourceFiles(TOAST_DIR);

	it('scans the toast sources', () => {
		expect(files.length).toBeGreaterThan(0);
	});

	it.each(files.map((file) => [file.slice(TOAST_DIR.length + 1)]))(
		'%s imports only allowed modules',
		(relative) => {
			const source = readFileSync(join(TOAST_DIR, relative), 'utf8');
			const forbidden = importSpecifiers(source).filter(
				(specifier) => !ALLOWED.some((rule) => rule.test(specifier)),
			);
			expect(forbidden).toEqual([]);
		},
	);
});
