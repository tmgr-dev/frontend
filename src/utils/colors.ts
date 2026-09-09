export interface Hsl {
	h: number;
	s: number;
	l: number;
}

/** `#rgb` or `#rrggbb` → HSL with integer degrees / percentages. */
export const hexToHsl = (hex: string): Hsl => {
	const raw = hex.replace('#', '');
	const full = raw.length === 3 ? raw.split('').map((c) => c + c).join('') : raw;
	const r = parseInt(full.slice(0, 2), 16) / 255;
	const g = parseInt(full.slice(2, 4), 16) / 255;
	const b = parseInt(full.slice(4, 6), 16) / 255;
	const max = Math.max(r, g, b);
	const min = Math.min(r, g, b);
	const delta = max - min;
	const l = (max + min) / 2;
	let h = 0;
	if (delta !== 0) {
		if (max === r) h = ((g - b) / delta) % 6;
		else if (max === g) h = (b - r) / delta + 2;
		else h = (r - g) / delta + 4;
		h = Math.round(h * 60);
		if (h < 0) h += 360;
	}
	const s = delta === 0 ? 0 : delta / (1 - Math.abs(2 * l - 1));
	return { h, s: Math.round(s * 100), l: Math.round(l * 100) };
};

/** HSL (degrees, percentages) → `#rrggbb`. */
export const hslToHex = (h: number, s: number, l: number): string => {
	const lightness = l / 100;
	const a = (s * Math.min(lightness, 1 - lightness)) / 100;
	const channel = (n: number) => {
		const k = (n + h / 30) % 12;
		const value = lightness - a * Math.max(Math.min(k - 3, 9 - k, 1), -1);
		return Math.round(255 * value)
			.toString(16)
			.padStart(2, '0');
	};
	return `#${channel(0)}${channel(8)}${channel(4)}`;
};
