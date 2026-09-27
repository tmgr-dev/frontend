const read = <T>(key: string, fallback: T): T => {
	try {
		const raw = localStorage.getItem(key);
		return raw === null ? fallback : (JSON.parse(raw) as T);
	} catch {
		return fallback;
	}
};

const write = (key: string, value: unknown) => {
	try {
		localStorage.setItem(key, JSON.stringify(value));
	} catch {
		// Settings stay in memory for this session.
	}
};

const ENABLED = 'plugins.enabled';
const SETTINGS = 'plugins.settings';
const SAFE_MODE = 'plugins.safeMode';
const DEV_MODE = 'plugins.devMode';
const MACHINE_CONSENT = 'plugins.machineConsent';
const ALARMS = 'plugins.alarms';
const DEEP_LINK_CONSENT = 'plugins.deepLinkConsent';
const TRAY_TITLE_PLUGIN = 'plugins.trayTitlePlugin';

/** Per workspace: `{ "<plugin>@<workspace id>": true | false }`. */
export const enabledStore = {
	get: (pluginId: string, workspaceId: number) =>
		read<Record<string, boolean>>(ENABLED, {})[`${pluginId}@${workspaceId}`],
	set: (pluginId: string, workspaceId: number, value: boolean) =>
		write(ENABLED, {
			...read<Record<string, boolean>>(ENABLED, {}),
			[`${pluginId}@${workspaceId}`]: value,
		}),
};

/** A member's consent that a plugin of a shared workspace may reach this computer, per member and pinned release. */
export const machineConsentStore = {
	has: (memberId: number, workspaceId: number, pluginId: string, release: string) =>
		read<Record<string, boolean>>(MACHINE_CONSENT, {})[
			`${memberId}:${pluginId}@${workspaceId}#${release}`
		] === true,
	set: (
		memberId: number,
		workspaceId: number,
		pluginId: string,
		release: string,
		value: boolean,
	) => {
		const all = read<Record<string, boolean>>(MACHINE_CONSENT, {});
		const key = `${memberId}:${pluginId}@${workspaceId}#${release}`;
		if (value) all[key] = true;
		else delete all[key];
		write(MACHINE_CONSENT, all);
	},
};

export const settingsStore = {
	get: (pluginId: string) =>
		read<Record<string, Record<string, unknown>>>(SETTINGS, {})[pluginId],
	set: (pluginId: string, values: Record<string, unknown>) =>
		write(SETTINGS, {
			...read<Record<string, Record<string, unknown>>>(SETTINGS, {}),
			[pluginId]: values,
		}),
};

export const safeModeStored = () => read<boolean>(SAFE_MODE, false) === true;
export const storeSafeMode = (value: boolean) => write(SAFE_MODE, value);
export const devModeStored = () => read<boolean>(DEV_MODE, false) === true;
export const storeDevMode = (value: boolean) => write(DEV_MODE, value);

/** Keyed by `${storageId}@${workspaceId}`; a plain plugin's storageId is its id, so the prefix still matches. */
export const alarmsStore = {
	get: (key: string) =>
		read<Record<string, Record<string, unknown>>>(ALARMS, {})[key],
	set: (key: string, defs: Record<string, unknown>) =>
		write(ALARMS, {
			...read<Record<string, Record<string, unknown>>>(ALARMS, {}),
			[key]: defs,
		}),
};

/** Per (plugin, command, plugin version): the user chose "Always" for a deep-linked command. */
export const deepLinkConsentStore = {
	has: (pluginId: string, commandId: string, version: string) =>
		read<Record<string, boolean>>(DEEP_LINK_CONSENT, {})[
			`${pluginId}:${commandId}@${version}`
		] === true,
	remember: (pluginId: string, commandId: string, version: string) =>
		write(DEEP_LINK_CONSENT, {
			...read<Record<string, boolean>>(DEEP_LINK_CONSENT, {}),
			[`${pluginId}:${commandId}@${version}`]: true,
		}),
};

/** The plugin id chosen in Settings to show text in the menu bar, or null when none is chosen. */
export const trayTitlePluginStore = {
	get: () => read<string | null>(TRAY_TITLE_PLUGIN, null),
	set: (pluginId: string | null) => write(TRAY_TITLE_PLUGIN, pluginId),
};

/** A removed plugin leaves nothing a later plugin with the same id could inherit. */
export const forgetPlugin = (pluginId: string) => {
	const enabled = read<Record<string, boolean>>(ENABLED, {});
	write(
		ENABLED,
		Object.fromEntries(
			Object.entries(enabled).filter(
				([key]) => !key.startsWith(`${pluginId}@`),
			),
		),
	);
	const settings = read<Record<string, Record<string, unknown>>>(SETTINGS, {});
	delete settings[pluginId];
	write(SETTINGS, settings);
	const alarms = read<Record<string, Record<string, unknown>>>(ALARMS, {});
	write(
		ALARMS,
		Object.fromEntries(
			Object.entries(alarms).filter(([key]) => !key.startsWith(`${pluginId}@`)),
		),
	);
	const consent = read<Record<string, boolean>>(DEEP_LINK_CONSENT, {});
	write(
		DEEP_LINK_CONSENT,
		Object.fromEntries(
			Object.entries(consent).filter(([key]) => !key.startsWith(`${pluginId}:`)),
		),
	);
	if (trayTitlePluginStore.get() === pluginId) trayTitlePluginStore.set(null);
};
