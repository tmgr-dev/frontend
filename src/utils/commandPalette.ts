import { ref } from 'vue';

export const commandPaletteOpen = ref(false);
let returnFocusTo: HTMLElement | null = null;
let canOpen: () => boolean = () => false;

export const setCommandPaletteAvailability = (check: () => boolean) => {
	canOpen = check;
};

export const openCommandPalette = () => {
	if (commandPaletteOpen.value || !canOpen()) return;
	const active =
		typeof document === 'undefined' ? null : document.activeElement;
	returnFocusTo =
		active && active !== document.body ? (active as HTMLElement) : null;
	commandPaletteOpen.value = true;
};

export const takeReturnFocus = (): HTMLElement | null => {
	const target = returnFocusTo;
	returnFocusTo = null;
	return target?.isConnected ? target : null;
};

interface WorkspaceRef {
	id: number | string;
	code: string;
}

export const resolvePaletteWorkspace = <T extends WorkspaceRef>(
	workspaces: T[] | null | undefined,
	routeCode: unknown,
	current: WorkspaceRef | null | undefined,
): T | null => {
	const list = workspaces ?? [];
	return (
		list.find((w) => typeof routeCode === 'string' && w.code === routeCode) ??
		list.find((w) => current != null && String(w.id) === String(current.id)) ??
		list[0] ??
		null
	);
};

export interface HotkeyEvent {
	key: string;
	code: string;
	metaKey: boolean;
	ctrlKey: boolean;
	shiftKey: boolean;
	altKey: boolean;
	repeat: boolean;
	defaultPrevented: boolean;
	target: EventTarget | null;
}

export const isCommandPaletteHotkey = (
	event: HotkeyEvent,
	isMac: boolean,
): boolean => {
	const isK = /^[a-z]$/i.test(event.key)
		? event.key.toLowerCase() === 'k'
		: event.code === 'KeyK';
	if (!isK || event.repeat) return false;
	if (event.shiftKey || event.altKey) return false;
	if (isMac ? !event.metaKey || event.ctrlKey : !event.ctrlKey || event.metaKey)
		return false;
	const target = event.target as Element | null;
	const inEditor =
		typeof target?.closest === 'function' && !!target.closest('.codex-editor');
	return !(event.defaultPrevented && inEditor);
};

export const hotkeyHint = (isMac: boolean): string => (isMac ? '⌘K' : 'Ctrl K');

const matchScore = (label: string, query: string): number => {
	const text = label.toLowerCase();
	if (text === query) return 0;
	if (text.startsWith(query)) return 1;
	if (text.split(/[\s\-_/]+/).some((word) => word.startsWith(query))) return 2;
	if (text.includes(query)) return 3;
	return -1;
};

export const rankByLabel = <T>(
	items: T[],
	query: string,
	label: (item: T) => string,
): T[] => {
	const q = query.trim().toLowerCase();
	if (!q) return items;
	return items
		.map((item, index) => ({ item, index, score: matchScore(label(item), q) }))
		.filter((entry) => entry.score >= 0)
		.sort((a, b) => a.score - b.score || a.index - b.index)
		.map((entry) => entry.item);
};

export interface NavTarget {
	key: string;
	label: string;
	to: string;
}

export interface NavFlags {
	dashboard: boolean;
	board: boolean;
	routines: boolean;
	localRoutines: boolean;
	categories: boolean;
	pages: boolean;
	files: boolean;
	map: boolean;
	aiUsage: boolean;
}

export const navTargets = (code: string, flags: NavFlags): NavTarget[] =>
	[
		flags.dashboard && {
			key: 'dashboard',
			label: 'Dashboard',
			to: `/${code}/dashboard`,
		},
		{ key: 'list', label: 'List', to: `/${code}/list` },
		flags.board && { key: 'board', label: 'Board', to: `/${code}/board` },
		flags.routines && {
			key: 'routines',
			label: flags.localRoutines ? 'Local routines' : 'Daily Routines',
			to: '/routines',
		},
		{ key: 'archive', label: 'Archive', to: `/${code}/archive` },
		flags.categories && {
			key: 'categories',
			label: 'Categories',
			to: `/${code}/categories`,
		},
		flags.pages && { key: 'pages', label: 'Pages', to: `/${code}/pages` },
		{ key: 'stats', label: 'Statistics', to: '/stats' },
		flags.aiUsage && { key: 'ai-usage', label: 'AI usage', to: '/ai-usage' },
		flags.files && { key: 'files', label: 'Files', to: `/${code}/files` },
		flags.map && { key: 'map', label: 'Map', to: `/${code}/map` },
		{ key: 'settings', label: 'Settings', to: '/settings' },
	].filter((item): item is NavTarget => !!item);

export interface RecentEntry {
	kind: 'task' | 'page';
	id: number;
	title: string;
	url: string;
	slug?: string;
}

export const RECENT_LIMIT = 8;

export const recentKey = (
	userId: number | string,
	workspaceId: number | string,
) => `tmgr:command-palette:recent:${userId}:${workspaceId}`;

export const pushRecent = (
	list: RecentEntry[],
	entry: RecentEntry,
	limit = RECENT_LIMIT,
): RecentEntry[] =>
	[
		entry,
		...list.filter((item) => item.kind !== entry.kind || item.id !== entry.id),
	].slice(0, limit);

const isRecentEntry = (value: any): value is RecentEntry =>
	(value?.kind === 'task' || value?.kind === 'page') &&
	Number.isFinite(value.id) &&
	typeof value.title === 'string' &&
	typeof value.url === 'string';

type KeyValueStorage = Pick<Storage, 'getItem' | 'setItem'>;

export const loadRecent = (
	storage: KeyValueStorage,
	userId: number | string,
	workspaceId: number | string,
): RecentEntry[] => {
	try {
		const parsed = JSON.parse(
			storage.getItem(recentKey(userId, workspaceId)) ?? '[]',
		);
		return Array.isArray(parsed)
			? parsed.filter(isRecentEntry).slice(0, RECENT_LIMIT)
			: [];
	} catch {
		return [];
	}
};

export const saveRecent = (
	storage: KeyValueStorage,
	userId: number | string,
	workspaceId: number | string,
	list: RecentEntry[],
) => {
	try {
		storage.setItem(recentKey(userId, workspaceId), JSON.stringify(list));
	} catch {
		/* quota or private mode: Recent is a convenience */
	}
};
