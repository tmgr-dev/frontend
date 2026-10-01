import { LOCAL_ID, PLUGIN_ID } from '@/pluginSystem/manifest';
import { AuthCallback, parseAuthCallback } from '@/utils/desktopAuth';
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

export type DeepLink =
	| AuthCallback
	| { type: 'task'; taskId: number }
	| { type: 'page'; workspaceCode: string; slug: string }
	| { type: 'view'; pluginId: string; viewId: string; params: Record<string, string> }
	| { type: 'command'; pluginId: string; commandId: string; params: Record<string, string> };

const MAX_DEEP_LINK_PARAMS = 20;
const MAX_DEEP_LINK_PARAM_VALUE = 500;
const PARAM_KEY = /^[a-zA-Z0-9_-]{1,40}$/;
const FORBIDDEN_PARAM_KEYS = new Set(['__proto__', 'constructor', 'prototype']);

/** Rejects duplicate keys, too many params, bad or forbidden keys, and oversized values. */
export const sanitizeDeepLinkParams = (
	entries: Iterable<[string, string]>,
): Record<string, string> | null => {
	const params: Record<string, string> = Object.create(null);
	let count = 0;
	for (const [key, value] of entries) {
		if (
			!PARAM_KEY.test(key) ||
			FORBIDDEN_PARAM_KEYS.has(key) ||
			value.length > MAX_DEEP_LINK_PARAM_VALUE
		)
			return null;
		if (key in params) return null;
		if (++count > MAX_DEEP_LINK_PARAMS) return null;
		params[key] = value;
	}
	return params;
};

const parseParams = (search: string): Record<string, string> | null => {
	let usp: URLSearchParams;
	try {
		usp = new URLSearchParams(search);
	} catch {
		return null;
	}
	return sanitizeDeepLinkParams(usp);
};

/** Matched on the raw string, before any percent-decoding, so `%2F` or `%2E` can never forge a segment. */
const PLUGIN_LINK =
	/^tmgr:\/\/plugin\/([^/?#]+)\/(view|command)\/([^/?#]+)(\?[^#]*)?$/;

const PAGE_LINK = /^tmgr:\/\/page\/([A-Za-z0-9_-]{1,64})\/([A-Za-z0-9_-]{1,120})\/?$/;

export const parseDeepLink = (url: string): DeepLink | null => {
	const trimmed = url.trim();
	const auth = parseAuthCallback(trimmed);
	if (auth) return auth;
	const taskMatch = /^tmgr:\/\/task\/(\d+)\/?$/.exec(trimmed);
	if (taskMatch) return { type: 'task', taskId: Number(taskMatch[1]) };

	const pageMatch = PAGE_LINK.exec(trimmed);
	if (pageMatch) {
		return { type: 'page', workspaceCode: pageMatch[1], slug: pageMatch[2] };
	}

	const match = PLUGIN_LINK.exec(trimmed);
	if (!match) return null;
	const [, pluginId, kind, targetId, search] = match;
	if (!PLUGIN_ID.test(pluginId)) return null;
	const params = parseParams(search ? search.slice(1) : '');
	if (!params) return null;
	if (kind === 'view') {
		return LOCAL_ID.test(targetId) ? { type: 'view', pluginId, viewId: targetId, params } : null;
	}
	return LOCAL_ID.test(targetId)
		? { type: 'command', pluginId, commandId: `${pluginId}.${targetId}`, params }
		: null;
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

export const pickQuickAddWorkspace = (
	workspaces: { id: number }[],
	remembered: number | null,
	current: number | null,
): number | null => {
	const has = (id: number | null) =>
		id !== null && workspaces.some((w) => w.id === id);
	if (has(remembered)) return remembered;
	if (has(current)) return current;
	return workspaces[0]?.id ?? null;
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

export interface GlobalShortcutApi {
	register(
		accelerator: string,
		handler: (event: { state: string }) => void,
	): Promise<void>;
	unregisterAll(): Promise<void>;
}

export const registerShortcuts = async (
	api: GlobalShortcutApi,
	config: ShortcutConfig,
	onPressed: (action: ShortcutAction) => void,
): Promise<{
	registered: string[];
	status: Record<ShortcutAction, ShortcutStatus>;
}> => {
	// Registrations outlive a page reload on the Rust side, still bound to the dead page's
	// callbacks, and would make every register below fail as already taken.
	await api.unregisterAll().catch(() => {});
	const registered: string[] = [];
	const status = {} as Record<ShortcutAction, ShortcutStatus>;
	for (const action of SHORTCUT_ACTIONS) {
		const { accelerator, enabled } = config[action];
		if (!enabled) {
			status[action] = 'off';
			continue;
		}
		try {
			await api.register(accelerator, (event) => {
				if (event.state === 'Pressed') onPressed(action);
			});
			registered.push(accelerator);
			status[action] = 'ok';
		} catch (error) {
			console.error(`shortcut ${accelerator} not registered`, error);
			status[action] = 'taken';
		}
	}
	return { registered, status };
};

const RECENT_URL_WINDOW_MS = 5000;

/** `getCurrent()` and `onOpenUrl` may both deliver the same cold-start URL; skip the second delivery. */
export const createRecentUrlGuard = (now: () => number = Date.now) => {
	let lastUrl: string | null = null;
	let lastAt = 0;
	return (url: string): boolean => {
		const at = now();
		if (lastUrl === url && at - lastAt < RECENT_URL_WINDOW_MS) return true;
		lastUrl = url;
		lastAt = at;
		return false;
	};
};
