import { reactive } from 'vue';
import type { PluginHost, PluginHostState } from './host';

/** Light, always-bundled part of the plugin system: what the UI reads. The host itself loads on desktop only. */
export const pluginState = reactive<PluginHostState>({
	workspace: null,
	safeMode: false,
	plugins: {},
	statusBar: {},
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
