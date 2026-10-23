import { computeThemeClasses } from '@/theme/applyTheme';
import {
	brandTextColor,
	buildPaletteCss,
	contrastRatio,
	onAccentColor,
} from '@/theme/paletteCss';
import { THEME_REGISTRY } from '@/theme/registry';
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
