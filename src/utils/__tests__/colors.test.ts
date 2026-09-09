import { hexToHsl, hslToHex } from '@/utils/colors';

const channels = (hex: string) =>
	[1, 3, 5].map((i) => parseInt(hex.slice(i, i + 2), 16));

describe('hexToHsl', () => {
	it('keeps saturation and lightness, not only the hue (TM-135)', () => {
		expect(hexToHsl('#1d4ed8')).toEqual({ h: 224, s: 76, l: 48 });
	});

	it('reads short hex', () => {
		expect(hexToHsl('#fff')).toEqual({ h: 0, s: 0, l: 100 });
	});

	it('treats greys as hue 0 with zero saturation', () => {
		expect(hexToHsl('#374151')).toEqual({ h: 217, s: 19, l: 27 });
		expect(hexToHsl('#808080')).toEqual({ h: 0, s: 0, l: 50 });
	});
});

describe('hslToHex', () => {
	it('round-trips a status colour within rounding error', () => {
		for (const hex of ['#1d4ed8', '#11224b', '#00a1ff', '#ff9100', '#75ff00', '#374151']) {
			const { h, s, l } = hexToHsl(hex);
			const back = channels(hslToHex(h, s, l));
			channels(hex).forEach((c, i) => expect(Math.abs(back[i] - c)).toBeLessThanOrEqual(2));
		}
	});

	it('renders pure hues at full saturation', () => {
		expect(hslToHex(0, 100, 50)).toBe('#ff0000');
		expect(hslToHex(120, 100, 50)).toBe('#00ff00');
	});
});
