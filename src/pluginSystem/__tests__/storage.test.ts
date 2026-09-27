import {
	deepLinkConsentStore,
	enabledStore,
	forgetPlugin,
	settingsStore,
	trayTitlePluginStore,
} from '../storage';

const data = new Map<string, string>();
(globalThis as any).localStorage = {
	getItem: (key: string) => data.get(key) ?? null,
	setItem: (key: string, value: string) => data.set(key, value),
	removeItem: (key: string) => data.delete(key),
};

it('forgets where a removed plugin was on and its settings, and nothing of other plugins', () => {
	enabledStore.set('acme.board', -1, true);
	enabledStore.set('acme.board', -2, false);
	enabledStore.set('acme.boardx', -1, true);
	settingsStore.set('acme.board', { key: 'secret' });
	settingsStore.set('other.plugin', { a: 1 });
	forgetPlugin('acme.board');
	expect(enabledStore.get('acme.board', -1)).toBeUndefined();
	expect(enabledStore.get('acme.board', -2)).toBeUndefined();
	expect(enabledStore.get('acme.boardx', -1)).toBe(true);
	expect(settingsStore.get('acme.board')).toBeUndefined();
	expect(settingsStore.get('other.plugin')).toEqual({ a: 1 });
});

it('remembers "Always" per plugin, command and version', () => {
	deepLinkConsentStore.remember('acme.board', 'acme.board.go', '1.0.0');
	expect(deepLinkConsentStore.has('acme.board', 'acme.board.go', '1.0.0')).toBe(true);
	expect(deepLinkConsentStore.has('acme.board', 'acme.board.go', '1.0.1')).toBe(false);
	expect(deepLinkConsentStore.has('acme.board', 'acme.board.other', '1.0.0')).toBe(false);
});

it('forgets a removed plugin\'s "Always" deep links and gives up the menu bar text if it held it', () => {
	deepLinkConsentStore.remember('acme.board', 'acme.board.go', '1.0.0');
	deepLinkConsentStore.remember('acme.boardx', 'acme.boardx.go', '1.0.0');
	trayTitlePluginStore.set('acme.board');
	forgetPlugin('acme.board');
	expect(deepLinkConsentStore.has('acme.board', 'acme.board.go', '1.0.0')).toBe(false);
	expect(deepLinkConsentStore.has('acme.boardx', 'acme.boardx.go', '1.0.0')).toBe(true);
	expect(trayTitlePluginStore.get()).toBeNull();
});

it('keeps the menu bar text choice when a different plugin is forgotten', () => {
	trayTitlePluginStore.set('acme.boardx');
	forgetPlugin('acme.board');
	expect(trayTitlePluginStore.get()).toBe('acme.boardx');
});
