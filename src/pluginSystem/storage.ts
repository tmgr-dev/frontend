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
