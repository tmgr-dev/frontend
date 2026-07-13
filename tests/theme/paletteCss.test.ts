import { buildPaletteCss } from '@/theme/paletteCss';
import { THEME_REGISTRY } from '@/theme/registry';

test('builds a rule per palette mapping bg and brand', () => {
	const css = buildPaletteCss(THEME_REGISTRY);
	expect(css).toContain('html.theme-dracula');
	expect(css).toContain('--bg:#282a36');
	expect(css).toContain('--brand-color:#bd93f9');
	expect(css).toContain('--timer-color:#50fa7b');
	expect((css.match(/html\.theme-/g) || []).length).toBe(THEME_REGISTRY.length);
});
