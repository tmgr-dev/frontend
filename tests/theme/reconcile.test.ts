import { pickThemeFromSettings } from '@/theme/reconcile';

test('extracts theme + colorScheme from settings array', () => {
	expect(
		pickThemeFromSettings([
			{ key: 'current_workspace', value: 3 },
			{ key: 'theme', value: 'nord' },
			{ key: 'colorScheme', value: 'dark' },
		]),
	).toEqual({ theme: 'nord', colorScheme: 'dark' });
});

test('returns empty when absent', () => {
	expect(pickThemeFromSettings([{ key: 'current_workspace', value: 3 }])).toEqual({});
	expect(pickThemeFromSettings(undefined)).toEqual({});
});
