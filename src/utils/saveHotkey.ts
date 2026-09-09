export interface SaveHotkeyEvent {
	type: string;
	code: string;
	ctrlKey: boolean;
	metaKey: boolean;
	repeat: boolean;
}

/**
 * Ctrl/Cmd+S, but only on the initial keydown. vueuse's useMagicKeys calls
 * onEventFired for keyup as well, and the modifier is still held when the S
 * key comes up, so without the type check the shortcut fires twice (TM-205).
 */
export const isSaveHotkey = (e: SaveHotkeyEvent): boolean =>
	e.type === 'keydown' && !e.repeat && (e.ctrlKey || e.metaKey) && e.code === 'KeyS';
