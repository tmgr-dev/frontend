import { humanizeGroupName, humanizeKey } from '@/utils/featureToggleCopy';
import { moduleNeedsAccount } from '@/utils/localOnly';

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
	local?: boolean;
	requires_account?: boolean;
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
	canHide: boolean;
	enforcement: string;
	packs: ModulePack[];
	presets: ModulePreset[];
	modules: ModuleEntry[];
	notices: string[];
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
		canHide: true,
		enforcement: body.enforcement || 'report',
		packs: Array.isArray(body.packs) ? body.packs : [],
		presets: Array.isArray(body.presets) ? body.presets : [],
		modules,
		notices: Array.isArray(body.notices) ? body.notices : [],
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

let lastRaw: string | null = null;
let lastParsed: ModulesMap | null = null;

export const readModulesCache = (workspaceId: unknown): ModulesMap | null => {
	if (workspaceId == null || workspaceId === '') return null;
	try {
		const raw = localStorage.getItem(cacheKey(workspaceId));
		if (raw === null) return null;
		if (raw === lastRaw) return lastParsed;
		const parsed = JSON.parse(raw);
		lastRaw = raw;
		lastParsed = parsed && typeof parsed === 'object' ? parsed : null;
		return lastParsed;
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
	needsAccount: boolean;
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
			canHide: !user && payload.canHide,
			askOwner: !user && !isOwner && !enabled,
			needsAccount: moduleNeedsAccount(entry),
		};
	};
	const coreItems = grouped.core.map((entry) => {
		const name = entry.name || humanizeKey(entry.key);
		const blocking = (entry.depends_on || [])
			.map((key) => map[key])
			.filter((dep) => dep && !dep.core && dep.enabled === false);
		return {
			name,
			needs: blocking.length
				? `needs ${blocking
						.map((dep) => dep.name || humanizeKey(dep.key))
						.join(', ')}`
				: '',
			off: blocking.length > 0,
		};
	});
	return {
		core: coreItems.map((item) => item.name),
		coreItems,
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
				text: 'Enable this feature',
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
		canHide: false,
		enforcement: 'report',
		packs: packKeys.map((key) => ({ key, name: humanizeGroupName(key) })),
		presets: [],
		modules,
		notices: [],
	};
};

export const CORE_DESCRIPTION =
	'Always available. You can hide any of them for yourself.';

export const hiddenBadge = (hidden: boolean): string | null =>
	hidden ? 'Hidden for you' : null;

export const hideButtonLabel = (hidden: boolean): string =>
	hidden ? 'Show for me' : 'Hide for me';

export const presetSummary = (
	preset: ModulePreset,
	modules: Pick<ModuleEntry, 'key' | 'name'>[],
): string => {
	if (preset.key === 'everything') return 'All modules';
	if (preset.modules.length === 0) return 'No modules, core only';
	const names = new Map(modules.map((m) => [m.key, m.name]));
	return preset.modules
		.filter((key) => !key.startsWith('notifications.'))
		.map((key) => names.get(key) || humanizeKey(key))
		.join(', ');
};

export const canApplyPreset = (canManage: boolean): boolean => canManage;

export const presetDiff = (
	preset: ModulePreset,
	modules: Pick<ModuleEntry, 'key' | 'name' | 'enabled' | 'core' | 'scope'>[],
): { on: string[]; off: string[] } => {
	const on: string[] = [];
	const off: string[] = [];
	for (const m of modules) {
		if (m.core || m.scope === 'user') continue;
		const target =
			preset.key === 'everything' || preset.modules.includes(m.key);
		const name = m.name || humanizeKey(m.key);
		if (target && m.enabled !== true) on.push(name);
		if (!target && m.enabled === true) off.push(name);
	}
	return { on, off };
};

export const payloadNotices = (payload: ModulesPayload): string[] =>
	payload.notices.filter(
		(notice): notice is string => typeof notice === 'string' && !!notice,
	);

export const WORKFLOW_OFF_CONFIRM =
	'Tasks get a Done checkbox instead of statuses; the Board is hidden. Statuses and their tasks stay and come back when you turn it on.';

export const workflowOffConfirm = (
	key: string,
	enabled: boolean,
): string | null =>
	key === 'workflow' && !enabled ? WORKFLOW_OFF_CONFIRM : null;

export const presetNotes = (
	preset: ModulePreset,
	modules: Pick<ModuleEntry, 'key' | 'enabled'>[],
): string[] => {
	const workflow = modules.find((m) => m.key === 'workflow');
	if (!workflow) return [];
	const target =
		preset.key === 'everything' || preset.modules.includes('workflow');
	const was = workflow.enabled === true;
	if (target === was) return [];
	return [
		target
			? 'Workflow: Done checkbox → statuses'
			: 'Workflow: statuses → Done checkbox',
	];
};
