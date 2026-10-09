import { humanizeGroupName, humanizeKey } from '@/utils/featureToggleCopy';

export interface ModuleEntry {
	key: string;
	name?: string;
	description?: string;
	pack?: string;
	clients?: string[];
	depends_on?: string[];
	scope?: 'workspace' | 'user';
	core?: boolean;
	enforced?: boolean;
	default?: boolean;
	enabled?: boolean;
	hidden?: boolean;
	value?: string | boolean;
	[extra: string]: unknown;
}

export interface ModulePack {
	key: string;
	name: string;
}

export interface ModulePreset {
	key: string;
	name: string;
	description: string;
	modules: string[];
}

export interface ModulesPayload {
	configured: boolean;
	canManage: boolean;
	enforcement: string;
	packs: ModulePack[];
	presets: ModulePreset[];
	modules: ModuleEntry[];
}

export type ModulesMap = Record<string, ModuleEntry>;

export const parseModulesPayload = (raw: any): ModulesPayload => {
	const body =
		raw && typeof raw === 'object' && raw.data && !Array.isArray(raw.data)
			? raw.data
			: raw || {};
	const modules = Array.isArray(body.modules)
		? body.modules
		: Object.entries(body.modules || {}).map(([key, entry]) => ({
				...(entry as object),
				key,
		  }));
	return {
		configured: body.configured !== false,
		canManage: body.can_manage === true,
		enforcement: body.enforcement || 'report',
		packs: Array.isArray(body.packs) ? body.packs : [],
		presets: Array.isArray(body.presets) ? body.presets : [],
		modules,
	};
};

export const modulesToMap = (payload: ModulesPayload): ModulesMap =>
	Object.fromEntries(payload.modules.map((entry) => [entry.key, entry]));

export const isEntryVisible = (entry?: ModuleEntry | null): boolean => {
	if (!entry) return true;
	if (entry.hidden === true) return false;
	if (entry.core) return true;
	if (entry.enabled === false) return false;
	if (entry.enabled === undefined && entry.value === false) return false;
	return true;
};

const cacheKey = (workspaceId: unknown) => `tmgr:modules:${workspaceId}`;

export const readModulesCache = (workspaceId: unknown): ModulesMap | null => {
	if (workspaceId == null || workspaceId === '') return null;
	try {
		const parsed = JSON.parse(
			localStorage.getItem(cacheKey(workspaceId)) || 'null',
		);
		return parsed && typeof parsed === 'object' ? parsed : null;
	} catch {
		return null;
	}
};

export const writeModulesCache = (
	workspaceId: unknown,
	map: ModulesMap,
): void => {
	if (workspaceId == null || workspaceId === '') return;
	try {
		localStorage.setItem(cacheKey(workspaceId), JSON.stringify(map));
	} catch {
		return;
	}
};

const pickerKey = (workspaceId: unknown) =>
	`tmgr:modules-picker:${workspaceId}`;

export const isPickerDismissed = (workspaceId: unknown): boolean => {
	try {
		return localStorage.getItem(pickerKey(workspaceId)) === '1';
	} catch {
		return false;
	}
};

export const dismissPicker = (workspaceId: unknown): void => {
	try {
		localStorage.setItem(pickerKey(workspaceId), '1');
	} catch {
		return;
	}
};

export interface ModulePackGroup {
	key: string;
	name: string;
	modules: ModuleEntry[];
	enabledCount: number;
	total: number;
}

export const groupModules = (payload: ModulesPayload) => {
	const core: ModuleEntry[] = [];
	const account: ModuleEntry[] = [];
	const byPack = new Map<string, ModuleEntry[]>();
	for (const entry of payload.modules) {
		if (entry.scope === 'user') account.push(entry);
		else if (entry.core) core.push(entry);
		else {
			const pack = entry.pack || 'other';
			byPack.set(pack, [...(byPack.get(pack) || []), entry]);
		}
	}
	const names = new Map(payload.packs.map((p) => [p.key, p.name]));
	const order = [
		...payload.packs.map((p) => p.key).filter((k) => byPack.has(k)),
		...[...byPack.keys()].filter((k) => !names.has(k)),
	].filter((key) => key !== 'core');
	const packs: ModulePackGroup[] = order.map((key) => {
		const modules = byPack.get(key) || [];
		return {
			key,
			name: names.get(key) || humanizeKey(key),
			modules,
			enabledCount: modules.filter((m) => m.enabled === true).length,
			total: modules.length,
		};
	});
	return { core, packs, account };
};

export const dependencyLabel = (
	entry: ModuleEntry,
	map: ModulesMap,
): string => {
	if (!entry.depends_on?.length) return '';
	return `needs: ${entry.depends_on
		.map((key) => map[key]?.name || humanizeKey(key))
		.join(', ')}`;
};

export interface ModuleRow {
	key: string;
	name: string;
	description: string;
	clients: string;
	needs: string;
	enabled: boolean;
	hidden: boolean;
	scope: 'workspace' | 'user';
	canToggle: boolean;
	canHide: boolean;
	askOwner: boolean;
}

export const buildModulesView = (payload: ModulesPayload, isOwner: boolean) => {
	const map = modulesToMap(payload);
	const grouped = groupModules(payload);
	const row = (entry: ModuleEntry): ModuleRow => {
		const user = entry.scope === 'user';
		const enabled = entry.enabled === true;
		return {
			key: entry.key,
			name: entry.name || humanizeKey(entry.key),
			description: entry.description || '',
			clients: (entry.clients || []).join(' · '),
			needs: dependencyLabel(entry, map),
			enabled,
			hidden: entry.hidden === true,
			scope: user ? 'user' : 'workspace',
			canToggle: user || isOwner,
			canHide: !user,
			askOwner: !user && !isOwner && !enabled,
		};
	};
	return {
		core: grouped.core.map((entry) => entry.name || humanizeKey(entry.key)),
		packs: grouped.packs.map((pack) => ({
			key: pack.key,
			name: pack.name,
			enabledCount: pack.enabledCount,
			total: pack.total,
			rows: pack.modules.map(row),
		})),
		account: grouped.account.map(row),
	};
};

export const gateCopy = (isOwner: boolean, hiddenByMe = false) => {
	if (hiddenByMe) {
		return {
			kind: 'link',
			text: 'Show this module',
			to: '/settings/modules',
		} as const;
	}
	return isOwner
		? ({
				kind: 'link',
				text: 'Enable this module',
				to: '/settings/modules',
		  } as const)
		: ({ kind: 'ask', text: 'Ask the owner to turn this on' } as const);
};

export const shouldShowPicker = (input: {
	configured: boolean;
	canManage: boolean;
	isOwner: boolean;
	isLocal: boolean;
	notFound: boolean;
}): boolean =>
	!input.configured &&
	!input.notFound &&
	!input.isLocal &&
	input.isOwner &&
	input.canManage;

export const featureDisabledMessage = (error: any): string | null => {
	const response = error?.response;
	if (response?.status !== 403 || response?.data?.error !== 'feature_disabled')
		return null;
	return typeof response.data.message === 'string'
		? response.data.message
		: null;
};

export const MODULES_FOOTER =
	'Turning a module off hides it for everyone in this workspace. Your data stays. Turn it back on and everything is where you left it.';

export const CUSTOM_CHOICE = 'custom';

export const buildChoiceRequest = (
	choice: string,
	custom: Record<string, boolean>,
): { preset: string } | { modules: Record<string, boolean> } =>
	choice === CUSTOM_CHOICE ? { modules: custom } : { preset: choice };

export const payloadFromLegacy = (
	map: ModulesMap,
	canManage: boolean,
	hiddenKeys: string[] = [],
): ModulesPayload => {
	const modules = Object.entries(map)
		.filter(
			([key, entry]) =>
				!hiddenKeys.includes(key) && entry.enabled !== undefined,
		)
		.map(([key, entry]) => ({
			...entry,
			key,
			scope: 'workspace' as const,
			pack: entry.group ? String(entry.group) : 'other',
		}));
	const packKeys = [...new Set(modules.map((m) => m.pack))];
	return {
		configured: true,
		canManage,
		enforcement: 'report',
		packs: packKeys.map((key) => ({ key, name: humanizeGroupName(key) })),
		presets: [],
		modules,
	};
};
