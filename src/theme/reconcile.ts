export function pickThemeFromSettings(
	settings: Array<{ key: string; value: string | number }> | undefined,
): { theme?: string; colorScheme?: string } {
	const out: { theme?: string; colorScheme?: string } = {};
	if (!Array.isArray(settings)) return out;
	for (const s of settings) {
		if (s.key === 'theme' && s.value != null) out.theme = String(s.value);
		if (s.key === 'colorScheme' && s.value != null)
			out.colorScheme = String(s.value);
	}
	return out;
}
