import { ThemeDef } from '@/theme/registry';

export function buildPaletteCss(registry: ThemeDef[]): string {
	return registry
		.map((t) => {
			const k = t.tokens;
			return (
				`html.theme-${t.id}{` +
				`--bg:${k.bg};--bg-raised:${k.surface};--bg-sunken:${k.bg};` +
				`--bg-hover:${k.raised};--bg-active:${k.raised};` +
				`--line-color:${k.border};--line-strong-color:${k.border};` +
				`--fg:${k.text};--fg-muted:${k.muted};--fg-subtle:${k.muted};--fg-faint:${k.muted};` +
				`--brand-color:${k.brand};--brand-bg-color:${k.raised};--brand-fg-color:${k.onAccent};` +
				`--brand-hover-color:${k.brand};--brand-ring-color:${k.brand};` +
				`--timer-color:${k.timer};--on-accent-color:${k.onAccent};` +
				`--tag-bg:${k.raised};--tag-fg:${k.muted};` +
				`}`
			);
		})
		.join('\n');
}
