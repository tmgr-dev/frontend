import { desktopOnly } from '../guard';

describe('desktopOnly', () => {
	afterEach(() => {
		delete (globalThis as any).window;
	});

	it('redirects home outside the desktop app', () => {
		(globalThis as any).window = {};
		expect(desktopOnly()).toBe('/');
	});

	it('lets the desktop app through', () => {
		(globalThis as any).window = { __TAURI_INTERNALS__: {} };
		expect(desktopOnly()).toBe(true);
	});
});
