import { deviceCanHover } from '../hoverCapability';

const media = (matches: boolean) => ({
	matchMedia: jest.fn(() => ({ matches }) as MediaQueryList),
});

describe('deviceCanHover', () => {
	it('is true with a mouse or trackpad', () => {
		const win = media(true);
		expect(deviceCanHover(win)).toBe(true);
		expect(win.matchMedia).toHaveBeenCalledWith('(hover: hover)');
	});

	it('is false on touch screens that cannot hover', () => {
		expect(deviceCanHover(media(false))).toBe(false);
	});

	it('assumes hover when matchMedia is missing', () => {
		expect(deviceCanHover(undefined)).toBe(true);
		expect(deviceCanHover({} as Pick<Window, 'matchMedia'>)).toBe(true);
	});
});
