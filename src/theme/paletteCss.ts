import { ThemeDef } from '@/theme/registry';

function hexToHslTriplet(hex: string): string {
	let h = hex.trim().replace('#', '');
	if (h.length === 3) {
		h = h[0] + h[0] + h[1] + h[1] + h[2] + h[2];
	}
	const r = parseInt(h.slice(0, 2), 16) / 255;
	const g = parseInt(h.slice(2, 4), 16) / 255;
	const b = parseInt(h.slice(4, 6), 16) / 255;
	const max = Math.max(r, g, b);
	const min = Math.min(r, g, b);
	const l = (max + min) / 2;
	let s = 0;
	let hue = 0;
	if (max !== min) {
		const d = max - min;
		s = l > 0.5 ? d / (2 - max - min) : d / (max + min);
		if (max === r) {
			hue = (g - b) / d + (g < b ? 6 : 0);
		} else if (max === g) {
			hue = (b - r) / d + 2;
		} else {
			hue = (r - g) / d + 4;
		}
		hue /= 6;
	}
	return `${Math.round(hue * 360)} ${Math.round(s * 100)}% ${Math.round(l * 100)}%`;
}

export function buildPaletteCss(registry: ThemeDef[]): string {
	return registry
		.map((t) => {
			const k = t.tokens;
			const bg = hexToHslTriplet(k.bg);
			const surface = hexToHslTriplet(k.surface);
			const raised = hexToHslTriplet(k.raised);
			const border = hexToHslTriplet(k.border);
			const text = hexToHslTriplet(k.text);
			const muted = hexToHslTriplet(k.muted);
			const brand = hexToHslTriplet(k.brand);
			const onAccent = hexToHslTriplet(k.onAccent);
			return (
				`html.theme-${t.id}{` +
				// design-token layer (surfaces, text, brand, status accents)
				`--bg:${k.bg};--bg-raised:${k.surface};--bg-sunken:${k.bg};` +
				`--bg-hover:${k.raised};--bg-active:${k.raised};` +
				`--line-color:${k.border};--line-strong-color:${k.border};` +
				`--fg:${k.text};--fg-muted:${k.muted};--fg-subtle:${k.muted};--fg-faint:${k.muted};` +
				`--brand-color:${k.brand};--brand-bg-color:${k.raised};--brand-fg-color:${k.onAccent};` +
				`--brand-hover-color:${k.brand};--brand-ring-color:${k.brand};` +
				`--timer-color:${k.timer};--on-accent-color:${k.onAccent};` +
				`--tag-bg:${k.raised};--tag-fg:${k.muted};` +
				// shadcn HSL layer (sidebar, cards, buttons, inputs, popovers, borders)
				`--background:${bg};--foreground:${text};` +
				`--card:${surface};--card-foreground:${text};` +
				`--popover:${surface};--popover-foreground:${text};` +
				`--primary:${brand};--primary-foreground:${onAccent};` +
				`--secondary:${raised};--secondary-foreground:${text};` +
				`--muted:${raised};--muted-foreground:${muted};` +
				`--accent:${raised};--accent-foreground:${text};` +
				`--border:${border};--input:${border};--ring:${brand};` +
				`--sidebar-background:${surface};--sidebar-foreground:${text};` +
				`--sidebar-primary:${brand};--sidebar-primary-foreground:${onAccent};` +
				`--sidebar-accent:${raised};--sidebar-accent-foreground:${text};` +
				`--sidebar-border:${border};--sidebar-ring:${brand};` +
				`}`
			);
		})
		.join('\n');
}
