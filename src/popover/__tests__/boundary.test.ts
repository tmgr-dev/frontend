import { readdirSync, readFileSync, statSync } from 'node:fs';
import { join } from 'node:path';

const POPOVER_DIR = join(__dirname, '..');

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
	/^@\/decisions\/api$/,
	/^@\/decisions\/types$/,
];

describe('popover import boundary', () => {
	const files = sourceFiles(POPOVER_DIR);

	it('scans the popover sources', () => {
		expect(files.length).toBeGreaterThan(0);
	});

	it.each(files.map((file) => [file.slice(POPOVER_DIR.length + 1)]))(
		'%s imports only allowed modules',
		(relative) => {
			const source = readFileSync(join(POPOVER_DIR, relative), 'utf8');
			const forbidden = importSpecifiers(source).filter(
				(specifier) => !ALLOWED.some((rule) => rule.test(specifier)),
			);
			expect(forbidden).toEqual([]);
		},
	);
});

describe('popover layout', () => {
	const template = readFileSync(
		join(POPOVER_DIR, 'components', 'App.vue'),
		'utf8',
	);

	it('places AI usage right after quick add and before the timer', () => {
		const quick = template.indexOf('class="quick"');
		const ai = template.indexOf("show('ai')");
		const timer = template.indexOf('<RunningTimer');
		expect(quick).toBeGreaterThan(-1);
		expect(ai).toBeGreaterThan(quick);
		expect(timer).toBeGreaterThan(ai);
	});

	it('wires header buttons to run(action.id) before Open TMGR', () => {
		const icons = template.indexOf('v-for="action in actions"');
		const open = template.indexOf("run('open')");
		expect(icons).toBeGreaterThan(-1);
		expect(icons).toBeLessThan(open);
		expect(template).toContain('@click="run(action.id)"');
	});
});
