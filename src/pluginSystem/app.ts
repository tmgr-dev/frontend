import { toast } from '@/components/ui/toast';
import { localWorkspaceById } from '@/local/runtime';
import { LOCAL_CODE_PREFIX } from '@/local/types';
import { domainEvents } from '@/utils/domainEvents';
import type { AxiosInstance } from 'axios';
import { watch } from 'vue';
import type { Store } from 'vuex';
import type { PluginWorkspace } from './broker';
import { builtinPackages } from './builtin';
import { createDataApi } from './dataApi';
import { createPluginHost, type PluginPackage } from './host';
import { parseManifest } from './manifest';
import type { WorkerEndpoint } from './process';
import {
	folderPluginErrors,
	pluginHost,
	pluginState,
	setPluginHost,
} from './state';
import {
	devModeStored,
	enabledStore,
	safeModeStored,
	settingsStore,
} from './storage';

export interface FolderPlugin {
	folder: string;
	manifest: string;
	code: string;
}

const folderPackages = async (): Promise<PluginPackage[]> => {
	if (!devModeStored()) return [];
	const { invoke } = await import('@tauri-apps/api/core');
	const found =
		(await invoke<FolderPlugin[]>('plugins_dev_list').catch(() => null)) ?? [];
	Object.keys(folderPluginErrors).forEach(
		(key) => delete folderPluginErrors[key],
	);
	return found.flatMap(({ folder, manifest, code }) => {
		try {
			return [
				{
					manifest: parseManifest(JSON.parse(manifest)),
					code,
					source: 'folder' as const,
				},
			];
		} catch (error) {
			folderPluginErrors[folder] =
				error instanceof Error ? error.message : String(error);
			return [];
		}
	});
};

const workspaceOf = (workspace: any): PluginWorkspace | null =>
	workspace && Number.isFinite(Number(workspace.id))
		? {
				id: Number(workspace.id),
				code: String(workspace.code),
				name: String(workspace.name),
				kind: Number(workspace.id) < 0 ? 'local' : 'cloud',
		  }
		: null;

/** The store knows the workspace object only after a workspace page loaded the list; a local one is found by id. */
const resolveWorkspace = async (
	store: Store<any>,
): Promise<PluginWorkspace | null> => {
	const known = workspaceOf(store.getters.currentWorkspace);
	if (known) return known;
	const id = Number(store.getters.currentWorkspaceId);
	if (!(id < 0)) return null;
	const local = await localWorkspaceById(id);
	return local
		? {
				id,
				code: `${LOCAL_CODE_PREFIX}${local.code}`,
				name: local.name,
				kind: 'local',
		  }
		: null;
};

/** Desktop only: starts the plugin host and follows the current workspace. */
export const installPlugins = async (
	http: AxiosInstance,
	store: Store<any>,
	safeModeFlag: boolean,
) => {
	pluginState.safeMode = safeModeFlag || safeModeStored();
	const host = createPluginHost({
		state: pluginState,
		packages: async () => {
			const builtins = builtinPackages();
			const ids = new Set(builtins.map((p) => p.manifest.id));
			return [
				...builtins,
				...(await folderPackages()).filter((p) => !ids.has(p.manifest.id)),
			];
		},
		createEndpoint: () =>
			new Worker(new URL('./worker.ts', import.meta.url), {
				type: 'module',
			}) as unknown as WorkerEndpoint,
		api: (pluginId) => createDataApi(http, pluginId),
		subscribe: (handler) => domainEvents.on(handler),
		enabled: enabledStore,
		settings: settingsStore,
		notify: (title, description) => toast({ title, description }),
		currentWorkspaceId: () => {
			const id = Number(store.getters.currentWorkspaceId);
			return Number.isFinite(id) && id !== 0 ? id : null;
		},
	});
	setPluginHost(host);
	watch(
		() =>
			Object.values(pluginState.plugins)
				.map(
					(p) => `${p.manifest.id} ${p.status}${p.error ? `: ${p.error}` : ''}`,
				)
				.join('\n'),
		async (now, before) => {
			const previous = new Set((before ?? '').split('\n'));
			const changed = now
				.split('\n')
				.filter((line) => line && !previous.has(line));
			if (!changed.length) return;
			const log = await import('@tauri-apps/plugin-log').catch(() => null);
			changed.forEach(
				(line) => void log?.info(`[plugin] ${line}`).catch(() => undefined),
			);
		},
	);
	await host.load();
	let sequence = 0;
	watch(
		() =>
			`${store.getters.currentWorkspaceId}|${
				store.getters.currentWorkspace?.code ?? ''
			}`,
		async () => {
			const current = ++sequence;
			const workspace = await resolveWorkspace(store);
			if (current !== sequence) return;
			const active = pluginState.workspace;
			if (active?.id === workspace?.id && active?.code === workspace?.code)
				return;
			await host.activate(workspace);
		},
		{ immediate: true },
	);
};

/** Re-reads folder plugins (developer mode) and restarts everything for the current workspace. */
export const reloadPlugins = async () => {
	const host = pluginHost();
	if (!host) return;
	await host.load();
	await host.activate(pluginState.workspace);
};
