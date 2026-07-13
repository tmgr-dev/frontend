import { computeThemeClasses } from '@/theme/applyTheme';

test('default theme uses colorScheme', () => {
	expect(computeThemeClasses('default', 'dark', false)).toEqual({ paletteClass: null, dark: true });
	expect(computeThemeClasses('default', 'default', false)).toEqual({ paletteClass: null, dark: false });
});

test('known dark palette forces dark + palette class', () => {
	expect(computeThemeClasses('dracula', 'default', false)).toEqual({ paletteClass: 'theme-dracula', dark: true });
});

test('light palette forces light', () => {
	expect(computeThemeClasses('github-light', 'dark', true)).toEqual({ paletteClass: 'theme-github-light', dark: false });
});

test('unknown theme falls back to default + colorScheme', () => {
	expect(computeThemeClasses('bogus', 'dark', false)).toEqual({ paletteClass: null, dark: true });
});

test('no stored prefs falls back to system', () => {
	expect(computeThemeClasses(null, null, true)).toEqual({ paletteClass: null, dark: true });
	expect(computeThemeClasses(undefined, undefined, false)).toEqual({ paletteClass: null, dark: false });
});
