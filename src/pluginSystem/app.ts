import { toast } from '@/components/ui/toast';
import $axios from '@/plugins/axios';
import { pinnedLocalClient } from '@/local/pinned';
import { localWorkspaceById } from '@/local/runtime';
import { LOCAL_CODE_PREFIX } from '@/local/types';
import { domainEvents, installDomainEvents } from '@/utils/domainEvents';
import type { AxiosInstance } from 'axios';
import { watch } from 'vue';
import type { Store } from 'vuex';
import type { PluginWorkspace } from './broker';
import { builtinPackages } from './builtin';
import { cloudPluginClient, type MintedToken } from './cloudClient';
import {
	hasMachineConsent,
	matchesPin,
	pinMismatch,
	pinOf,
	recordFor,
	releaseOf,
	workspacePlugins,
	BUILTIN_REPO,
	type WorkspacePluginRecord,
} from './cloud';
import { createDataApi } from './dataApi';
import { encodeFile } from './fileData';
import { folderPackagesFrom, type FolderPlugin } from './folder';
import { createPluginHost, type PluginPackage } from './host';
import {
	blockedById,
	pluginCatalog,
	REFRESH_MS,
	setCatalog,
	type Catalog,
} from './catalog';
import { bundleToPackage, type Release } from './market';
import type { WorkerEndpoint } from './process';
import {
	folderPluginErrors,
	installedPluginErrors,
	pluginHost,
	pluginState,
	setPluginHost,
} from './state';
import {
	devModeStored,
	enabledStore,
	forgetPlugin,
	machineConsentStore,
	safeModeStored,
	settingsStore,
} from './storage';

const installedBlocked = new Map<string, string>();

/** Installed from GitHub; Rust drops any whose bundle no longer matches the checksum agreed to. */
const installedPackages = async (): Promise<PluginPackage[]> => {
	const { invoke } = await import('@tauri-apps/api/core');
	const found = await invoke<(Release & { id: string })[]>(
		'plugins_installed_list',
	).catch(() => null);
	// Keep the last known blocks: a failed listing must not unblock what is already loaded.
	if (!found) return [];
	Object.keys(installedPluginErrors).forEach(
		(key) => delete installedPluginErrors[key],
	);
	installedBlocked.clear();
	found.forEach((release) => {
		if (release.blocked) installedBlocked.set(release.id, release.blocked);
	});
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

const pickFile = async (pluginName: string) => {
	const { invoke } = await import('@tauri-apps/api/core');
	const picked = await invoke<{
		name: string;
		size: number;
		base64: string;
	} | null>('plugin_pick_file', {
		title: `Choose a file for the ${pluginName} plugin`,
	});
	if (!picked) return null;
	const bytes = Uint8Array.from(atob(picked.base64), (c) => c.charCodeAt(0));
	return { name: picked.name, ...encodeFile(bytes, null, picked.name) };
};

const cloudClients = new Map<string, AxiosInstance>();

/** Plugin data in a shared workspace: its own client and short-lived plugin token, never the user's. */
const cloudClientFor = (workspaceId: number, pluginId: string) => {
	const key = `${workspaceId}:${pluginId}`;
	let client = cloudClients.get(key);
	if (!client) {
		client = cloudPluginClient({
			baseURL: String($axios.defaults.baseURL ?? ''),
			workspaceId,
			mint: async () =>
				(
					await $axios.post<{ data: MintedToken }>(
						`workspaces/${workspaceId}/plugins/${pluginId}/token`,
					)
				).data.data,
		});
		installDomainEvents(client, domainEvents, () => workspaceId);
		cloudClients.set(key, client);
	}
	return client;
};

const loadWorkspacePlugins = async (workspaceId: number) => {
	const { data } = await $axios.get<{ data: WorkspacePluginRecord[] }>(
		`workspaces/${workspaceId}/plugins`,
	);
	workspacePlugins[workspaceId] = data.data ?? [];
};

/** Members get what the creator pinned: that exact release, checked against the pinned checksum and key. */
const syncWorkspacePlugins = async (workspaceId: number) => {
	const { invoke } = await import('@tauri-apps/api/core');
	let installedAny = false;
	const errors: Record<string, string> = {};
	for (const record of workspacePlugins[workspaceId] ?? []) {
		if (record.repo === BUILTIN_REPO) continue;
		const existing = pluginState.plugins[record.plugin_id];
		if (matchesPin(record, existing)) continue;
		if (existing) {
			// The member's own install is theirs: never replace it (with other permissions or an older
			// release) behind their back. The pinned release simply does not run until they match.
			errors[record.plugin_id] = `Shared workspace plugin: this workspace runs ${
				record.repo
			} ${record.version}, but ${
				existing.source === 'installed'
					? `you have ${existing.origin?.tag ?? 'another release'} installed`
					: `a ${existing.source} plugin uses this id`
			}. Remove yours to use the workspace's release.`;
			continue;
		}
		try {
			const release = await invoke<Release>('plugin_github_release', {
				repo: record.repo,
				tag: record.version,
			});
			const problem =
				pinMismatch(record, release) ??
				installConflict(
					release,
					bundleToPackage(release).manifest.id,
					record.plugin_id,
				);
			if (problem) throw new Error(problem);
			await invoke('plugin_install', {
				plugin: { id: record.plugin_id, ...release },
			});
			installedAny = true;
		} catch (error) {
			errors[record.plugin_id] = `Shared workspace plugin: ${
				error instanceof Error ? error.message : String(error)
			}`;
		}
	}
	if (installedAny) await pluginHost()?.load();
	Object.assign(installedPluginErrors, errors);
};

const workspaceOf = (workspace: any): PluginWorkspace | null =>
	workspace && Number.isFinite(Number(workspace.id))
		? {
				id: Number(workspace.id),
				code: String(workspace.code),
				name: String(workspace.name),
				kind: Number(workspace.id) < 0 ? 'local' : 'cloud',
				ownerId:
					workspace.user_id == null ? undefined : Number(workspace.user_id),
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
	generation: string;
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
						payload.generation,
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
		api: (pluginId, workspace, storageId) =>
			workspace.kind === 'cloud'
				? createDataApi(cloudClientFor(workspace.id, pluginId), pluginId, pluginId)
				: createDataApi(clientFor(workspace.id, store), pluginId, storageId),
		subscribe: (handler) => domainEvents.on(handler),
		enabled: {
			get: (pluginId, workspaceId) => {
				if (workspaceId < 0) return enabledStore.get(pluginId, workspaceId);
				const record = recordFor(workspaceId, pluginId);
				return !!record && matchesPin(record, pluginState.plugins[pluginId]);
			},
			set: (pluginId, workspaceId, value) => {
				if (workspaceId < 0) enabledStore.set(pluginId, workspaceId, value);
			},
		},
		machineAllowed: (pluginId, workspace) =>
			hasMachineConsent(workspace, pluginId),
		settings: settingsStore,
		notify: (title, description) => toast({ title, description }),
		files: (pluginId, workspace, pluginName) => {
			const code = workspace.code.replace(LOCAL_CODE_PREFIX, '');
			const folder = `plugins/${pluginId}`;
			const invoke = async <T>(
				command: string,
				args: Record<string, unknown>,
			) => (await import('@tauri-apps/api/core')).invoke<T>(command, args);
			if (workspace.kind === 'cloud') {
				const localOnly = async () => {
					throw new Error('exports go to a local workspace folder; this one is shared');
				};
				return {
					export: localOnly,
					reveal: localOnly,
					pick: async () => pickFile(pluginName),
				};
			}
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
				pick: async () => pickFile(pluginName),
			};
		},
		windows: {
			open: async (key, html, title, generation) => {
				const { invoke } = await import('@tauri-apps/api/core');
				await invoke('plugin_page_put', { key, html });
				await invoke('plugin_window_open', { key, title, generation });
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
		blocked: (pluginId) =>
			installedBlocked.get(pluginId) ?? blockedById(pluginCatalog, pluginId),
	});
	setPluginHost(host);
	const { invoke } = await import('@tauri-apps/api/core');
	setCatalog(
		await invoke<Catalog>('plugin_catalog').catch(() => pluginCatalog),
	);
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
	const refreshCatalog = async () => {
		const next = await invoke<Catalog>('plugin_catalog_refresh').catch(
			() => null,
		);
		if (!next) return;
		setCatalog(next);
		await host.load();
		host.applyBlocklist();
	};
	void refreshCatalog();
	setInterval(() => void refreshCatalog(), REFRESH_MS);
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
			if (workspace?.kind === 'cloud') {
				await loadWorkspacePlugins(workspace.id).catch(() => undefined);
				await syncWorkspacePlugins(workspace.id);
				if (current !== sequence) return;
			}
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

/** Why a release may not be installed over what is there: another plugin's id, or another repository. */
export const installConflict = (
	release: Release,
	manifestId: string,
	expectedId?: string,
) => {
	if (expectedId && manifestId !== expectedId) {
		return `this release is ${manifestId}, not ${expectedId}`;
	}
	const existing = pluginState.plugins[manifestId];
	if (!existing) return null;
	if (existing.source !== 'installed') {
		return `${manifestId} is already used by a ${
			existing.source === 'builtin' ? 'built-in' : 'folder'
		} plugin`;
	}
	if (existing.origin?.repo !== release.repo) {
		return `${manifestId} is already installed from github.com/${existing.origin?.repo}`;
	}
	return null;
};

export const installRelease = async (release: Release, expectedId?: string) => {
	const { invoke } = await import('@tauri-apps/api/core');
	const { manifest } = bundleToPackage(release);
	const conflict = installConflict(release, manifest.id, expectedId);
	if (conflict) throw new Error(conflict);
	await invoke('plugin_install', { plugin: { id: manifest.id, ...release } });
	await reloadPlugins();
};

export const uninstallPlugin = async (pluginId: string) => {
	const { invoke } = await import('@tauri-apps/api/core');
	await invoke('plugin_uninstall', { id: pluginId });
	forgetPlugin(pluginId);
	pluginHost()?.forget(pluginId);
};

/** The creator of a shared workspace turns a plugin on or off there, for every member. */
export const setWorkspacePlugin = async (pluginId: string, on: boolean) => {
	const workspace = pluginState.workspace;
	const entry = pluginState.plugins[pluginId];
	if (!workspace || workspace.kind !== 'cloud' || !entry) return;
	if (on) {
		const pin = pinOf(entry);
		if (!pin) throw new Error('only built-in and installed plugins can be shared');
		await $axios.put(`workspaces/${workspace.id}/plugins/${pluginId}`, pin);
	} else {
		await $axios.delete(`workspaces/${workspace.id}/plugins/${pluginId}`);
	}
	await loadWorkspacePlugins(workspace.id);
	if (pluginState.workspace?.id === workspace.id)
		await pluginHost()?.activate(workspace);
};

/** A member lets a shared workspace's plugin reach this computer (network, files), or takes it back. */
export const setMachineConsent = async (pluginId: string, allowed: boolean) => {
	const workspace = pluginState.workspace;
	if (!workspace || workspace.kind !== 'cloud') return;
	const record = recordFor(workspace.id, pluginId);
	if (!record) return;
	machineConsentStore.set(workspace.id, pluginId, releaseOf(record), allowed);
	await pluginHost()?.restart(pluginId);
};

/** Reads the shared workspace's plugin list again (the creator may have changed it). */
export const refreshWorkspacePlugins = async () => {
	const workspace = pluginState.workspace;
	if (!workspace || workspace.kind !== 'cloud') return;
	await loadWorkspacePlugins(workspace.id);
	await syncWorkspacePlugins(workspace.id);
	if (pluginState.workspace?.id === workspace.id)
		await pluginHost()?.activate(workspace);
};
