import { THEME_IS_DARK, DEFAULT_THEME, THEME_REGISTRY } from '@/theme/registry';
import { buildPaletteCss } from '@/theme/paletteCss';

export function computeThemeClasses(
	theme: string | null | undefined,
	colorScheme: string | null | undefined,
	prefersDark: boolean,
): { paletteClass: string | null; dark: boolean } {
	const isKnownPalette =
		!!theme && theme !== DEFAULT_THEME && theme in THEME_IS_DARK;
	if (isKnownPalette) {
		return { paletteClass: `theme-${theme}`, dark: THEME_IS_DARK[theme as string] };
	}
	const hasScheme = colorScheme === 'dark' || colorScheme === 'default';
	const dark = hasScheme ? colorScheme === 'dark' : prefersDark;
	return { paletteClass: null, dark };
}

export function ensurePaletteStyleInjected(doc: Document = document): void {
	if (doc.getElementById('tmgr-theme-palettes')) return;
	const style = doc.createElement('style');
	style.id = 'tmgr-theme-palettes';
	style.textContent = buildPaletteCss(THEME_REGISTRY);
	doc.head.appendChild(style);
}

export function applyThemeToDocument(
	theme: string | null | undefined,
	colorScheme: string | null | undefined,
	doc: Document = document,
): void {
	ensurePaletteStyleInjected(doc);
	const prefersDark =
		typeof doc.defaultView?.matchMedia === 'function' &&
		doc.defaultView.matchMedia('(prefers-color-scheme: dark)').matches;
	const { paletteClass, dark } = computeThemeClasses(theme, colorScheme, !!prefersDark);
	const html = doc.documentElement;
	Array.from(html.classList)
		.filter((c) => c.startsWith('theme-'))
		.forEach((c) => html.classList.remove(c));
	if (paletteClass) html.classList.add(paletteClass);
	html.classList.toggle('dark', dark);
}
