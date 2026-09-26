import { enabledStore, forgetPlugin, settingsStore } from '../storage';

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
