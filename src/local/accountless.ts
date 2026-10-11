import { CURRENT_WORKSPACE } from './patch';

export const LOCAL_USER_ID = -1;

export const LOCAL_SETTING_IDS = {
	[CURRENT_WORKSPACE]: 9001,
	theme: 9002,
	colorScheme: 9003,
} as const;

const SETTINGS_KEY = 'local.account.settings';
const TOGGLES_KEY = 'local.account.toggles';
const PREFS_KEY = 'local.account.preferences';

export const REQUIRES_ACCOUNT = {
	message: 'This needs a TMGR account',
	code: 'requires_account',
};

const LANDINGS = ['list', 'board', 'dashboard', 'daily_routines'];

const read = (key: string): Record<string, any> => {
	try {
		const raw = localStorage.getItem(key);
		const value = raw ? JSON.parse(raw) : null;
		return value && typeof value === 'object' && !Array.isArray(value) ? value : {};
	} catch {
		return {};
	}
};

const write = (key: string, value: Record<string, any>) => {
	try {
		localStorage.setItem(key, JSON.stringify(value));
	} catch {
		/* storage unavailable: the choice lasts until restart */
	}
};

const stored = (key: string, fallback: string): string => {
	try {
		return localStorage.getItem(key) || fallback;
	} catch {
		return fallback;
	}
};

export const localSettings = (workspaceId: number | null) => {
	const saved = read(SETTINGS_KEY);
	return [
		{
			id: LOCAL_SETTING_IDS[CURRENT_WORKSPACE],
			key: CURRENT_WORKSPACE,
			name: 'Current workspace',
			value: workspaceId,
			description: '',
		},
		{
			id: LOCAL_SETTING_IDS.theme,
			key: 'theme',
			name: 'Theme',
			value: saved.theme ?? stored('theme', 'default'),
			description: '',
		},
		{
			id: LOCAL_SETTING_IDS.colorScheme,
			key: 'colorScheme',
			name: 'Color scheme',
			value: saved.colorScheme ?? stored('colorScheme', 'system'),
			description: '',
		},
	];
};

export const localIdentity = (name: string, workspaceId: number | null) => ({
	id: LOCAL_USER_ID,
	name,
	email: '',
	role: 0,
	is_local: true,
	settings: localSettings(workspaceId),
});

/** Applies a `PUT v2/user/settings` payload; the current-workspace entry is the caller's business. */
export const saveLocalSettings = (payload: unknown) => {
	if (!Array.isArray(payload)) return;
	const saved = read(SETTINGS_KEY);
	for (const entry of payload) {
		if (entry?.id === LOCAL_SETTING_IDS.theme) saved.theme = String(entry.value);
		if (entry?.id === LOCAL_SETTING_IDS.colorScheme) saved.colorScheme = String(entry.value);
	}
	write(SETTINGS_KEY, saved);
};

export const requestedLocalWorkspace = (payload: unknown): number | null => {
	if (!Array.isArray(payload)) return null;
	const entry = payload.find((item) => item?.id === LOCAL_SETTING_IDS[CURRENT_WORKSPACE]);
	return entry && entry.value !== null && entry.value !== undefined ? Number(entry.value) : null;
};

export const userToggleDefaults = { default_landing_page: 'list', mcp: true };

export const readUserToggles = (): { default_landing_page: string; mcp: boolean } => {
	const saved = read(TOGGLES_KEY);
	return {
		default_landing_page: LANDINGS.includes(saved.default_landing_page)
			? saved.default_landing_page
			: userToggleDefaults.default_landing_page,
		mcp: typeof saved.mcp === 'boolean' ? saved.mcp : userToggleDefaults.mcp,
	};
};

export const userFeatureToggles = () => {
	const toggles = readUserToggles();
	return {
		default_landing_page: {
			key: 'default_landing_page',
			name: 'Default landing page',
			group: 'account',
			type: 'select',
			value: toggles.default_landing_page,
			default: userToggleDefaults.default_landing_page,
			options: LANDINGS,
		},
		mcp: { key: 'mcp', name: 'MCP & API tokens', group: 'account', enabled: toggles.mcp },
	};
};

export const saveUserToggles = (body: any) => {
	const features = body?.features;
	if (!features || typeof features !== 'object') return;
	const saved = read(TOGGLES_KEY);
	if (LANDINGS.includes(features.default_landing_page)) {
		saved.default_landing_page = features.default_landing_page;
	}
	if (typeof features.mcp === 'boolean') saved.mcp = features.mcp;
	write(TOGGLES_KEY, saved);
};

export const readPreferences = () => ({ showTooltips: true, ...read(PREFS_KEY) });

export const savePreferences = (body: any) => {
	const incoming = body?.settings && typeof body.settings === 'object' ? body.settings : body;
	if (!incoming || typeof incoming !== 'object' || Array.isArray(incoming)) return;
	write(PREFS_KEY, { ...read(PREFS_KEY), ...incoming });
};
