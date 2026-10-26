import { computeThemeClasses } from '@/theme/applyTheme';
import {
	brandTextColor,
	buildPaletteCss,
	contrastRatio,
	hslTripletToHex,
	mutedTextColor,
	onAccentColor,
} from '@/theme/paletteCss';
import { THEME_REGISTRY } from '@/theme/registry';
import { execFileSync } from 'child_process';
import { readFileSync } from 'fs';
import { join } from 'path';

const MIN = 4.5;

const rule = (id: string) => {
	const css = buildPaletteCss(THEME_REGISTRY);
	const m = css.match(new RegExp(`html\\.theme-${id}\\{([^}]*)\\}`));
	return m ? m[1] : '';
};
const decl = (body: string, name: string) =>
	(body.match(new RegExp(`${name}:([^;]+);`)) || [])[1];

describe.each(THEME_REGISTRY.map((t) => [t.id, t] as const))('%s', (id, t) => {
	const k = t.tokens;

	test('brand-fg is brand text readable on bg, surface and raised', () => {
		const fg = decl(rule(id), '--brand-fg-color');
		expect(fg).toBe(brandTextColor(t));
		for (const ground of [k.bg, k.surface, k.raised]) {
			expect(contrastRatio(fg, ground)).toBeGreaterThanOrEqual(MIN);
		}
	});

	test('brand-fg stays the brand colour where the brand already reads', () => {
		const plain = [k.bg, k.surface, k.raised].every(
			(g) => contrastRatio(k.brand, g) >= MIN,
		);
		expect(decl(rule(id), '--brand-fg-color')).toBe(
			plain ? k.brand : brandTextColor(t),
		);
		expect(decl(rule(id), '--brand-fg-color')).not.toBe(k.onAccent);
	});

	test('secondary text tokens read on bg, surface and raised', () => {
		const body = rule(id);
		const muted = mutedTextColor(t);
		expect(decl(body, '--fg-muted')).toBe(muted);
		expect(decl(body, '--fg-subtle')).toBe(muted);
		expect(decl(body, '--tag-fg')).toBe(muted);
		expect(decl(body, '--fg-faint')).toBe(k.muted);
		const fromTriplet = hslTripletToHex(decl(body, '--muted-foreground'));
		for (const ground of [k.bg, k.surface, k.raised]) {
			expect(contrastRatio(muted, ground)).toBeGreaterThanOrEqual(MIN);
			expect(contrastRatio(fromTriplet, ground)).toBeGreaterThanOrEqual(MIN);
		}
	});

	test('on-accent text reads on the brand fill', () => {
		const onAccent = decl(rule(id), '--on-accent-color');
		expect(onAccent).toBe(onAccentColor(t));
		expect(contrastRatio(onAccent, k.brand)).toBeGreaterThanOrEqual(MIN);
	});
});

test('themes whose own onAccent fails 4.5:1 fall back to black or white', () => {
	const failing = THEME_REGISTRY.filter(
		(t) => contrastRatio(t.tokens.onAccent, t.tokens.brand) < MIN,
	).map((t) => t.id);
	expect(failing).toEqual(['monokai', 'solarized-dark']);
	for (const t of THEME_REGISTRY.filter((x) => failing.includes(x.id))) {
		expect(['#000000', '#ffffff']).toContain(onAccentColor(t));
	}
});

test('themes with a failing brand colour get a pulled-in brand-fg', () => {
	const adjusted = THEME_REGISTRY.filter(
		(t) => brandTextColor(t) !== t.tokens.brand,
	).map((t) => t.id);
	expect(adjusted).toEqual(['monokai', 'solarized-dark', 'github-light']);
});

describe('default and unknown themes use the :root token defaults', () => {
	test('no palette rule exists for the default theme or an unknown id', () => {
		const css = buildPaletteCss(THEME_REGISTRY);
		expect(css).not.toContain('html.theme-default');
		expect(css).not.toContain('html.theme-bogus');
		expect(
			computeThemeClasses('default', 'dark', false).paletteClass,
		).toBeNull();
		expect(computeThemeClasses('bogus', 'dark', false).paletteClass).toBeNull();
	});

	test('light and dark default define on-accent white', () => {
		const scss = readFileSync(
			join(__dirname, '../../src/assets/styles/_design-tokens.scss'),
			'utf8',
		);
		const light = scss.slice(
			scss.indexOf(':root {'),
			scss.indexOf('html.dark {'),
		);
		const dark = scss.slice(scss.indexOf('html.dark {'));
		expect(light).toMatch(/--on-accent-color:\s*#fff;/);
		expect(dark).toMatch(/--on-accent-color:\s*#fff;/);
	});
});

describe('default light and dark secondary text tokens', () => {
	const scss = readFileSync(
		join(__dirname, '../../src/assets/styles/_design-tokens.scss'),
		'utf8',
	);
	const light = scss.slice(
		scss.indexOf(':root {'),
		scss.indexOf('html.dark {'),
	);
	const dark = scss.slice(scss.indexOf('html.dark {'));
	const val = (block: string, name: string) =>
		(block.match(new RegExp(`${name}:\\s*(#[0-9a-fA-F]{6});`)) || [])[1];

	test.each([
		['light', light],
		['dark', dark],
	])('%s fg-muted and fg-subtle read on bg, raised and sunken', (_n, block) => {
		for (const token of ['--fg-muted', '--fg-subtle']) {
			for (const ground of ['--bg', '--bg-raised', '--bg-sunken']) {
				expect(
					contrastRatio(val(block, token), val(block, ground)),
				).toBeGreaterThanOrEqual(MIN);
			}
		}
	});
});

test('no text-ink-faint class is left in src', () => {
	let hits = '';
	try {
		hits = execFileSync(
			'grep',
			['-rn', 'text-ink-faint', join(__dirname, '../../src')],
			{ encoding: 'utf8' },
		);
	} catch (e) {
		if ((e as { status?: number }).status !== 1) throw e;
	}
	expect(hits).toBe('');
});
