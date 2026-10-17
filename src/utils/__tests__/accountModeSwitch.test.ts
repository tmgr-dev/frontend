import {
	shouldCloseDetachedWindow,
	shouldReloadOnModeChange,
} from '../accountModeSwitch';

describe('shouldReloadOnModeChange', () => {
	it('reloads when moving between local-only and cloud', () => {
		expect(shouldReloadOnModeChange('local', 'cloud')).toBe(true);
		expect(shouldReloadOnModeChange('cloud', 'local')).toBe(true);
	});

	it('does not reload for anything else', () => {
		expect(shouldReloadOnModeChange('none', 'cloud')).toBe(false);
		expect(shouldReloadOnModeChange('none', 'local')).toBe(false);
		expect(shouldReloadOnModeChange('cloud', 'none')).toBe(false);
		expect(shouldReloadOnModeChange('local', 'none')).toBe(false);
		expect(shouldReloadOnModeChange('cloud', 'cloud')).toBe(false);
	});
});

describe('shouldCloseDetachedWindow', () => {
	it('closes when the cloud session ends even though the app stays usable', () => {
		expect(
			shouldCloseDetachedWindow(
				{ isLoggedIn: true, canUseApp: true },
				{ isLoggedIn: false, canUseApp: true },
			),
		).toBe(true);
	});

	it('closes when the app can no longer be used', () => {
		expect(
			shouldCloseDetachedWindow(
				{ isLoggedIn: false, canUseApp: true },
				{ isLoggedIn: false, canUseApp: false },
			),
		).toBe(true);
	});

	it('stays open otherwise', () => {
		expect(
			shouldCloseDetachedWindow(
				{ isLoggedIn: false, canUseApp: true },
				{ isLoggedIn: true, canUseApp: true },
			),
		).toBe(false);
		expect(
			shouldCloseDetachedWindow(
				{ isLoggedIn: false, canUseApp: true },
				{ isLoggedIn: false, canUseApp: true },
			),
		).toBe(false);
	});
});
