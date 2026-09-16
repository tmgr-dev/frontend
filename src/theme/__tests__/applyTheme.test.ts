import { isDarkTheme } from '@/theme/applyTheme';

describe('isDarkTheme', () => {
	it('is dark for a dark palette even when the color scheme is default', () => {
		expect(isDarkTheme('dracula', 'default', false)).toBe(true);
	});

	it('is light for a light palette even when the color scheme is dark', () => {
		expect(isDarkTheme('github-light', 'dark', false)).toBe(false);
	});

	it('follows the color scheme when no palette is chosen', () => {
		expect(isDarkTheme('default', 'dark', false)).toBe(true);
		expect(isDarkTheme('default', 'default', true)).toBe(false);
	});

	it('follows the system preference when neither is set', () => {
		expect(isDarkTheme(null, null, true)).toBe(true);
		expect(isDarkTheme(null, null, false)).toBe(false);
	});
});
