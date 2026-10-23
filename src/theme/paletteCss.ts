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
	return `${Math.round(hue * 360)} ${Math.round(s * 100)}% ${Math.round(
		l * 100,
	)}%`;
}

function channels(hex: string): number[] {
	let h = hex.trim().replace('#', '');
	if (h.length === 3) {
		h = h[0] + h[0] + h[1] + h[1] + h[2] + h[2];
	}
	return [0, 2, 4].map((i) => parseInt(h.slice(i, i + 2), 16));
}

function luminance(hex: string): number {
	const [r, g, b] = channels(hex).map((v) => {
		const c = v / 255;
		return c <= 0.03928 ? c / 12.92 : Math.pow((c + 0.055) / 1.055, 2.4);
	});
	return 0.2126 * r + 0.7152 * g + 0.0722 * b;
}

export function contrastRatio(a: string, b: string): number {
	const [hi, lo] = [luminance(a), luminance(b)].sort((x, y) => y - x);
	return (hi + 0.05) / (lo + 0.05);
}

const BRAND_TEXT_MIN_CONTRAST = 4.5;

// Brand-coloured text sits on bg, surface and raised: where the plain brand colour reads
// below 4.5:1 on one of them, it is pulled toward the theme text colour just far enough.
export function brandTextColor(t: ThemeDef): string {
	const { brand, bg, surface, raised, text } = t.tokens;
	const grounds = [bg, surface, raised];
	const ok = (c: string) =>
		grounds.every((g) => contrastRatio(c, g) >= BRAND_TEXT_MIN_CONTRAST);
	const from = channels(brand);
	const to = channels(text);
	for (let step = 0; step <= 20; step++) {
		const mix =
			'#' +
			from
				.map((v, i) => Math.round(v + ((to[i] - v) * step) / 20))
				.map((v) => v.toString(16).padStart(2, '0'))
				.join('');
		if (ok(mix)) return mix;
	}
	return text;
}

// Text on a brand fill: the theme's own onAccent unless it reads below 4.5:1, then black or white.
export function onAccentColor(t: ThemeDef): string {
	const { brand, onAccent } = t.tokens;
	if (contrastRatio(onAccent, brand) >= BRAND_TEXT_MIN_CONTRAST)
		return onAccent;
	return contrastRatio('#000000', brand) >= contrastRatio('#ffffff', brand)
		? '#000000'
		: '#ffffff';
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
			const brandFg = brandTextColor(t);
			const onAccentHex = onAccentColor(t);
			const onAccent = hexToHslTriplet(onAccentHex);
			return (
				`html.theme-${t.id}{` +
				// design-token layer (surfaces, text, brand, status accents)
				`--bg:${k.bg};--bg-raised:${k.surface};--bg-sunken:${k.bg};` +
				`--bg-hover:${k.raised};--bg-active:${k.raised};` +
				`--line-color:${k.border};--line-strong-color:${k.border};` +
				`--fg:${k.text};--fg-muted:${k.muted};--fg-subtle:${k.muted};--fg-faint:${k.muted};` +
				`--brand-color:${k.brand};--brand-bg-color:${k.raised};--brand-fg-color:${brandFg};` +
				`--brand-hover-color:${k.brand};--brand-ring-color:${k.brand};` +
				`--timer-color:${k.timer};--on-accent-color:${onAccentHex};` +
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
