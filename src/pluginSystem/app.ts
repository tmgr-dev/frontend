import { toast } from '@/components/ui/toast';
import { pinnedLocalClient } from '@/local/pinned';
import { localWorkspaceById } from '@/local/runtime';
import { LOCAL_CODE_PREFIX } from '@/local/types';
import { domainEvents, installDomainEvents } from '@/utils/domainEvents';
import type { AxiosInstance } from 'axios';
import { reactive, watch } from 'vue';
import type { Store } from 'vuex';
import type { PluginWorkspace } from './broker';
import { builtinPackages } from './builtin';
import { createDataApi } from './dataApi';
import { encodeFile } from './fileData';
import { folderPackagesFrom, type FolderPlugin } from './folder';
import { createPluginHost, type PluginPackage } from './host';
import { bundleToPackage, type Release } from './market';
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

export const installedPluginErrors = reactive<Record<string, string>>({});

/** Installed from GitHub; Rust drops any whose bundle no longer matches the checksum agreed to. */
const installedPackages = async (): Promise<PluginPackage[]> => {
	const { invoke } = await import('@tauri-apps/api/core');
	const found =
		(await invoke<(Release & { id: string })[]>('plugins_installed_list').catch(
			() => null,
		)) ?? [];
	Object.keys(installedPluginErrors).forEach(
		(key) => delete installedPluginErrors[key],
	);
	return found.flatMap((release) => {
		try {
			return [bundleToPackage(release)];
		} catch (error) {
			installedPluginErrors[release.id] =
				error instanceof Error ? error.message : String(error);
			return [];
		}
	});
};

const folderPackages = async (
	reservedIds: string[],
): Promise<PluginPackage[]> => {
	if (!devModeStored()) return [];
	const { invoke } = await import('@tauri-apps/api/core');
	const found =
		(await invoke<FolderPlugin[]>('plugins_dev_list').catch(() => null)) ?? [];
	const { packages, errors } = folderPackagesFrom(found, reservedIds);
	Object.keys(folderPluginErrors).forEach(
		(key) => delete folderPluginErrors[key],
	);
	Object.assign(folderPluginErrors, errors);
	return packages;
};

const clients = new Map<number, AxiosInstance>();

/** Plugin data never goes through the app's client: one client per local workspace, no network at all. */
const clientFor = (workspaceId: number, store: Store<any>) => {
	let client = clients.get(workspaceId);
	if (!client) {
		client = pinnedLocalClient(workspaceId, () => {
			const user = store.state.user ?? {};
			return {
				id: Number(user.id ?? 0),
				name: user.name ?? '',
				email: user.email ?? '',
			};
		});
		installDomainEvents(client, domainEvents, () => workspaceId);
		clients.set(workspaceId, client);
	}
	return client;
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

interface WindowCall {
	call_id: number;
	plugin_id: string;
	method: string;
	params: unknown;
}

/** Plugin windows reach the app only through this: Rust names the plugin by its window, never the page. */
const answerPluginWindows = async (
	host: ReturnType<typeof createPluginHost>,
) => {
	const [{ listen }, { invoke }] = await Promise.all([
		import('@tauri-apps/api/event'),
		import('@tauri-apps/api/core'),
	]);
	await listen<WindowCall>('plugin-window://call', async ({ payload }) => {
		let ok = true;
		let value: unknown;
		try {
			value = JSON.parse(
				JSON.stringify(
					(await host.windowCall(
						payload.plugin_id,
						payload.method,
						payload.params,
					)) ?? null,
				),
			);
		} catch (error: any) {
			ok = false;
			value = `${error?.code ?? 'ERROR'}: ${error?.message ?? error}`.slice(
				0,
				1000,
			);
		}
		await invoke('plugin_window_reply', {
			callId: payload.call_id,
			ok,
			value,
		}).catch(() => undefined);
	});
};

/** Desktop only: starts the plugin host and follows the current workspace. */
export const installPlugins = async (
	store: Store<any>,
	safeModeFlag: boolean,
) => {
	pluginState.safeMode = safeModeFlag || safeModeStored();
	const host = createPluginHost({
		state: pluginState,
		packages: async () => {
			const builtins = builtinPackages();
			const installed = (await installedPackages()).filter(
				(p) => !builtins.some((b) => b.manifest.id === p.manifest.id),
			);
			const reserved = [...builtins, ...installed].map((p) => p.manifest.id);
			return [...builtins, ...installed, ...(await folderPackages(reserved))];
		},
		createEndpoint: () =>
			new Worker(new URL('./worker.ts', import.meta.url), {
				type: 'module',
			}) as unknown as WorkerEndpoint,
		api: (pluginId, workspace) =>
			createDataApi(clientFor(workspace.id, store), pluginId),
		subscribe: (handler) => domainEvents.on(handler),
		enabled: enabledStore,
		settings: settingsStore,
		notify: (title, description) => toast({ title, description }),
		files: (pluginId, workspace, pluginName) => {
			const code = workspace.code.replace(LOCAL_CODE_PREFIX, '');
			const folder = `plugins/${pluginId}`;
			const invoke = async <T>(
				command: string,
				args: Record<string, unknown>,
			) => (await import('@tauri-apps/api/core')).invoke<T>(command, args);
			return {
				export: async (path, content) => {
					await invoke('local_export_write', {
						code,
						folder,
						files: [{ path, content }],
					});
					return { path };
				},
				reveal: async (path) => {
					await invoke('local_reveal', {
						code,
						relative: `exports/${folder}/${path}`,
					});
				},
				pick: async () => {
					const picked = await invoke<{
						name: string;
						size: number;
						base64: string;
					} | null>('plugin_pick_file', {
						title: `Choose a file for the ${pluginName} plugin`,
					});
					if (!picked) return null;
					const bytes = Uint8Array.from(atob(picked.base64), (c) =>
						c.charCodeAt(0),
					);
					return { name: picked.name, ...encodeFile(bytes, null, picked.name) };
				},
			};
		},
		windows: {
			open: async (key, html, title) => {
				const { invoke } = await import('@tauri-apps/api/core');
				await invoke('plugin_page_put', { key, html });
				await invoke('plugin_window_open', { key, title });
			},
			close: async (pluginId) => {
				const { invoke } = await import('@tauri-apps/api/core');
				await invoke('plugin_windows_close', { pluginId });
			},
		},
		fetch: async (request) => {
			const { invoke } = await import('@tauri-apps/api/core');
			return invoke('plugin_fetch', { request });
		},
		currentWorkspaceId: () => {
			const id = Number(store.getters.currentWorkspaceId);
			return Number.isFinite(id) && id !== 0 ? id : null;
		},
	});
	setPluginHost(host);
	await answerPluginWindows(host);
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

/** Reads the latest GitHub release of a plugin and checks it; nothing is installed until the user agrees. */
export const fetchRelease = async (repo: string) => {
	const { invoke } = await import('@tauri-apps/api/core');
	const release = await invoke<Release>('plugin_github_release', { repo });
	return { release, pkg: bundleToPackage(release) };
};

export const installRelease = async (release: Release) => {
	const { invoke } = await import('@tauri-apps/api/core');
	const { manifest } = bundleToPackage(release);
	await invoke('plugin_install', { plugin: { id: manifest.id, ...release } });
	await reloadPlugins();
};

export const uninstallPlugin = async (pluginId: string) => {
	const { invoke } = await import('@tauri-apps/api/core');
	await invoke('plugin_uninstall', { id: pluginId });
	pluginHost()?.forget(pluginId);
};
