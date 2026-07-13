import { THEME_REGISTRY, THEME_MAP, THEME_IS_DARK, DEFAULT_THEME } from '@/theme/registry';

test('registry has 17 palettes with complete tokens', () => {
	expect(THEME_REGISTRY).toHaveLength(17);
	for (const t of THEME_REGISTRY) {
		expect(t.id).toMatch(/^[a-z0-9-]+$/);
		expect(typeof t.isDark).toBe('boolean');
		for (const k of ['bg', 'surface', 'raised', 'border', 'text', 'muted', 'brand', 'timer', 'onAccent'] as const) {
			expect(t.tokens[k]).toMatch(/^#|^rgb|^oklch/i);
		}
	}
});

test('lookup maps are consistent', () => {
	expect(THEME_MAP['dracula'].name).toBe('Dracula');
	expect(THEME_IS_DARK['github-light']).toBe(false);
	expect(THEME_IS_DARK['dracula']).toBe(true);
	expect(DEFAULT_THEME).toBe('default');
});
