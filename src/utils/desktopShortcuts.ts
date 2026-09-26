import { ref } from 'vue';

export type ShortcutAction = 'quickAdd' | 'timer' | 'screenshot' | 'selection';

export interface ShortcutSetting {
	accelerator: string;
	enabled: boolean;
}

export type ShortcutConfig = Record<ShortcutAction, ShortcutSetting>;

export type ShortcutStatus = 'ok' | 'taken' | 'off';

export const SHORTCUT_ACTIONS: ShortcutAction[] = [
	'quickAdd',
	'timer',
	'screenshot',
	'selection',
];

export const DEFAULT_SHORTCUTS: ShortcutConfig = {
	quickAdd: { accelerator: 'Alt+Space', enabled: true },
	timer: { accelerator: 'Alt+Shift+T', enabled: true },
	screenshot: { accelerator: 'Alt+Shift+S', enabled: true },
	selection: { accelerator: 'Alt+Shift+C', enabled: true },
};

const PRIMARY_MODIFIERS = ['Command', 'Control', 'Alt'];
const RESERVED = [
	'Command+Space',
	'Command+Tab',
	'Command+Q',
	'Command+W',
	'Command+H',
	'Command+M',
	'Command+C',
	'Command+V',
	'Command+X',
	'Command+Z',
	'Command+A',
	'Command+Shift+3',
	'Command+Shift+4',
	'Command+Shift+5',
	'Control+Space',
];
const SYMBOLS: Record<string, string> = {
	Command: '⌘',
	Control: '⌃',
	Alt: '⌥',
	Shift: '⇧',
};

const keyFromCode = (code: string): string | null => {
	if (/^Key[A-Z]$/.test(code)) return code.slice(3);
	if (/^Digit[0-9]$/.test(code)) return code.slice(5);
	if (/^F([1-9]|1[0-9])$/.test(code)) return code;
	const named: Record<string, string> = {
		Space: 'Space',
		Enter: 'Enter',
		Backquote: '`',
		Minus: '-',
		Equal: '=',
		BracketLeft: '[',
		BracketRight: ']',
		Semicolon: ';',
		Quote: "'",
		Comma: ',',
		Period: '.',
		Slash: '/',
		Backslash: '\\',
		ArrowUp: 'Up',
		ArrowDown: 'Down',
		ArrowLeft: 'Left',
		ArrowRight: 'Right',
	};
	return named[code] ?? null;
};

export const eventToAccelerator = (event: KeyboardEvent): string | null => {
	const key = keyFromCode(event.code);
	if (!key) return null;
	const mods = [
		event.metaKey && 'Command',
		event.ctrlKey && 'Control',
		event.altKey && 'Alt',
		event.shiftKey && 'Shift',
	].filter(Boolean) as string[];
	return [...mods, key].join('+');
};

export const validateAccelerator = (
	accelerator: string,
): 'needs-modifier' | 'reserved' | null => {
	const parts = accelerator.split('+');
	if (!parts.some((p) => PRIMARY_MODIFIERS.includes(p))) {
		return 'needs-modifier';
	}
	return RESERVED.includes(accelerator) ? 'reserved' : null;
};

export const findConflict = (
	config: ShortcutConfig,
	action: ShortcutAction,
	accelerator: string,
): ShortcutAction | null =>
	SHORTCUT_ACTIONS.find(
		(other) =>
			other !== action &&
			config[other].enabled &&
			config[other].accelerator === accelerator,
	) ?? null;

export const mergeShortcuts = (
	stored: Partial<ShortcutConfig>,
): ShortcutConfig =>
	SHORTCUT_ACTIONS.reduce(
		(config, action) => ({
			...config,
			[action]: { ...DEFAULT_SHORTCUTS[action], ...(stored?.[action] || {}) },
		}),
		{} as ShortcutConfig,
	);

export const describeAccelerator = (accelerator: string): string =>
	accelerator
		.split('+')
		.map((part) => SYMBOLS[part] ?? part)
		.join('');

export const parseDeepLink = (url: string): { taskId: number } | null => {
	const match = /^tmgr:\/\/task\/(\d+)\/?$/.exec(url.trim());
	return match ? { taskId: Number(match[1]) } : null;
};

export const splitQuickText = (
	text: string,
): { title: string; note: string } => {
	const trimmed = text.trim();
	const firstLine = trimmed.split('\n')[0].trim();
	if (firstLine === trimmed && trimmed.length <= 120) {
		return { title: trimmed, note: '' };
	}
	const title =
		firstLine.length > 120 ? `${firstLine.slice(0, 119)}…` : firstLine;
	return { title, note: trimmed };
};

const STORAGE_KEY = 'desktop.shortcuts';

const loadShortcuts = (): ShortcutConfig => {
	try {
		return mergeShortcuts(JSON.parse(localStorage.getItem(STORAGE_KEY) || '{}'));
	} catch {
		return mergeShortcuts({});
	}
};

export const shortcutConfig = ref<ShortcutConfig>(
	typeof window === 'undefined' ? mergeShortcuts({}) : loadShortcuts(),
);

export const shortcutStatus = ref<Partial<Record<ShortcutAction, ShortcutStatus>>>(
	{},
);

export const saveShortcuts = (config: ShortcutConfig): void => {
	shortcutConfig.value = config;
	try {
		localStorage.setItem(STORAGE_KEY, JSON.stringify(config));
	} catch {
		/* storage unavailable: settings last until restart */
	}
};
