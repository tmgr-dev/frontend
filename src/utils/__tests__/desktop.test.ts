import {
	desktopPlatform,
	hasNativeTitleBar,
	revealInFileManagerLabel,
	supportsLocalAccess,
	supportsSelectionCapture,
	thisComputerName,
	trayName,
} from '../desktop';

describe('desktopPlatform', () => {
	it.each([
		[
			{
				platform: 'MacIntel',
				userAgent: 'Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7)',
			},
			'macos',
		],
		[{ platform: 'darwin' }, 'macos'],
		[
			{
				platform: 'Win32',
				userAgent: 'Mozilla/5.0 (Windows NT 10.0; Win64; x64)',
			},
			'windows',
		],
		[{ platform: 'win32' }, 'windows'],
		[
			{
				platform: 'Linux x86_64',
				userAgent: 'Mozilla/5.0 (X11; Linux x86_64)',
			},
			'linux',
		],
		[{ userAgent: 'Mozilla/5.0 (X11; Ubuntu; Linux x86_64)' }, 'linux'],
	] as const)('detects %j as %s', (nav, expected) => {
		expect(desktopPlatform(nav)).toBe(expected);
	});

	it('returns null when nothing identifies the platform', () => {
		expect(desktopPlatform({})).toBeNull();
		expect(desktopPlatform({ platform: '', userAgent: 'curl/8' })).toBeNull();
	});
});

describe('platform copy and capabilities', () => {
	it('names the tray per platform', () => {
		expect(trayName('macos')).toBe('menu bar');
		expect(trayName('windows')).toBe('system tray');
		expect(trayName('linux')).toBe('system tray');
	});

	it('names the file manager action per platform', () => {
		expect(revealInFileManagerLabel('macos')).toBe('Show in Finder');
		expect(revealInFileManagerLabel('windows')).toBe('Show in Explorer');
		expect(revealInFileManagerLabel('linux')).toBe('Show in file manager');
	});

	it('names the computer per platform', () => {
		expect(thisComputerName('macos')).toBe('this Mac');
		expect(thisComputerName('windows')).toBe('this computer');
	});

	it('gates features by platform', () => {
		expect(supportsSelectionCapture('macos')).toBe(true);
		expect(supportsSelectionCapture('windows')).toBe(false);
		expect(supportsSelectionCapture('linux')).toBe(false);
		expect(supportsLocalAccess('macos')).toBe(true);
		expect(supportsLocalAccess('linux')).toBe(true);
		expect(supportsLocalAccess('windows')).toBe(false);
		expect(hasNativeTitleBar('macos')).toBe(false);
		expect(hasNativeTitleBar('windows')).toBe(true);
		expect(hasNativeTitleBar('linux')).toBe(true);
	});
});
