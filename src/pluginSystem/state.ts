import { reactive } from 'vue';
import type { LocalConnectionResult } from './broker';
import type { PluginHost, PluginHostState } from './host';

/** Light, always-bundled part of the plugin system: what the UI reads. The host itself loads on desktop only. */
export const pluginState = reactive<PluginHostState>({
	workspace: null,
	safeMode: false,
	plugins: {},
	statusBar: {},
	trayItems: {},
	trayTitle: null,
	viewBadges: {},
	revision: 0,
	revisions: {},
});

export const folderPluginErrors = reactive<Record<string, string>>({});
export const installedPluginErrors = reactive<Record<string, string>>({});

let host: PluginHost | null = null;
export const pluginHost = () => host;
export const setPluginHost = (value: PluginHost | null) => {
	host = value;
};

export interface LocalAccessConnectRequest {
	pluginId: string;
	pluginName: string;
	label?: string;
	permissions?: string[];
}

/**
 * A plugin's tmgr.localAccess.requestConnection is waiting for the UI to open (or fall back for)
 * LocalPersonaConnectDialog. Settings/App shell renders it from here and answers via
 * resolveLocalAccessConnect; the promise given to the plugin resolves only then.
 */
export const localAccessConnect = reactive<{ current: LocalAccessConnectRequest | null }>({
	current: null,
});

let resolveConnect: ((result: LocalConnectionResult) => void) | null = null;

export const requestLocalAccessConnect = (
	request: LocalAccessConnectRequest,
): Promise<LocalConnectionResult> =>
	new Promise((resolve) => {
		resolveConnect?.({ status: 'cancelled' });
		resolveConnect = resolve;
		localAccessConnect.current = request;
	});

export const resolveLocalAccessConnect = (result: LocalConnectionResult): void => {
	localAccessConnect.current = null;
	const resolve = resolveConnect;
	resolveConnect = null;
	resolve?.(result);
};
