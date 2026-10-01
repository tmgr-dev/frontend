import { readdirSync, readFileSync, statSync } from 'fs';
import { join, relative } from 'path';

const ROOT = join(__dirname, '../../../..');
const CYRILLIC = /[Ѐ-ӿ]/;

const TARGETS = [
	'src/components/pages',
	'src/components/pagesNav',
	'src/pages',
	'src/utils/pages',
	'src/utils/pagesTree.ts',
	'src/utils/pagesSearch.ts',
	'src/composable/usePagesActions.ts',
	'src/local/pages',
];

const ALLOWLIST = new Set([
	'src/local/pages/slug.ts',
	'src/utils/pages/headingAliases.ts',
]);

const isSource = (path: string, name: string) =>
	/\.(ts|js|vue)$/.test(name) &&
	!path.includes('__tests__') &&
	!/\.(test|spec)\./.test(name);

const collect = (path: string, out: string[]) => {
	const stat = statSync(path);
	if (stat.isDirectory()) {
		for (const name of readdirSync(path)) collect(join(path, name), out);
		return out;
	}
	const name = path.split('/').pop() as string;
	const rel = relative(ROOT, path);
	if (!isSource(rel, name)) return out;
	if (rel.startsWith('src/pages/') && !/^Page/.test(name)) return out;
	out.push(rel);
	return out;
};

describe('Pages UI stays English', () => {
	const files = TARGETS.flatMap((target) => collect(join(ROOT, target), []));

	it('scans the Pages sources', () => {
		expect(files.length).toBeGreaterThan(30);
	});

	it('has no Cyrillic outside the slug and alias tables', () => {
		const offenders = files.filter(
			(file) =>
				!ALLOWLIST.has(file) &&
				CYRILLIC.test(readFileSync(join(ROOT, file), 'utf8')),
		);
		expect(offenders).toEqual([]);
	});
});
