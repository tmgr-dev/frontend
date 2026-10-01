import { ToastAction, toast } from '@/components/ui/toast';
import { usePusher } from '@/composable/usePusher';
import $axios from '@/plugins/axios';
import { pinnedLocalClient } from '@/local/pinned';
import { localWorkspaceById } from '@/local/runtime';
import { LOCAL_CODE_PREFIX } from '@/local/types';
import { domainEvents, installDomainEvents } from '@/utils/domainEvents';
import {
	dndState,
	expireDndIfNeeded,
	isDndActive,
	startDndClock,
	syncDndToTray,
} from '@/utils/dnd';
import type { AxiosInstance } from 'axios';
import { h, watch } from 'vue';
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
import { stepDevWatch, type DevWatchState } from './devReload';
import { encodeFile } from './fileData';
import { folderPackagesFrom, type FolderPlugin } from './folder';
import { createPluginHost, storageIdOf, type PluginPackage } from './host';
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
	requestLocalAccessConnect,
	setPluginHost,
} from './state';
import {
	alarmsStore,
	devModeStored,
	enabledStore,
	forgetPlugin,
	machineConsentStore,
	safeModeStored,
	settingsStore,
	trayTitlePluginStore,
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

const DEV_WATCH_INTERVAL_MS = 2000;

/**
 * Developer mode only: while the window is visible, polls each folder plugin's on-disk fingerprint and
 * restarts just the ones that changed (their storage survives, since it lives on disk). `reset()` forces
 * the next poll to reseed instead of restarting, so a change made while this was not the active host (or
 * before a manual reload) is not treated as a hot-reload event on its own.
 */
const startDevPluginWatcher = (host: ReturnType<typeof createPluginHost>) => {
	let watch: DevWatchState = null;
	let polling = false;
	const poll = async () => {
		if (polling || !devModeStored() || document.visibilityState !== 'visible') return;
		polling = true;
		try {
			const { invoke } = await import('@tauri-apps/api/core');
			const found = await invoke<{ folder: string; fingerprint: string }[]>(
				'plugins_dev_fingerprints',
			).catch(() => null);
			if (!found) return;
			const fingerprints = Object.fromEntries(found.map((f) => [f.folder, f.fingerprint]));
			const { next, changed } = stepDevWatch(watch, fingerprints);
			watch = next;
			if (!changed.length) return;
			const list = (await invoke<FolderPlugin[]>('plugins_dev_list').catch(() => null)) ?? [];
			const idByFolder = new Map<string, string>();
			for (const plugin of list) {
				try {
					idByFolder.set(plugin.folder, JSON.parse(plugin.manifest).id);
				} catch {
					// an invalid manifest is already surfaced by the normal folderPackages() error list
				}
			}
			await host.load();
			for (const folder of changed) {
				const pluginId = idByFolder.get(folder);
				if (pluginId) await host.restart(pluginId);
			}
		} finally {
			polling = false;
		}
	};
	const timer = setInterval(() => void poll(), DEV_WATCH_INTERVAL_MS);
	return {
		reset: () => (watch = null),
		dispose: () => clearInterval(timer),
	};
};

let devPluginWatcher: ReturnType<typeof startDevPluginWatcher> | null = null;

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
	view_id: string;
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
						payload.view_id,
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

let memberId = () => 0;

let markPluginsReady: (() => void) | null = null;
let pluginsReadyPromise: Promise<void> | null = null;
let pluginsReadyDone = false;

/** Resolves once plugins have started (or given up starting); callers after that point resolve immediately. */
export const pluginsReady = (): Promise<void> => {
	if (pluginsReadyDone) return Promise.resolve();
	if (!pluginsReadyPromise) {
		pluginsReadyPromise = new Promise((resolve) => {
			markPluginsReady = resolve;
		});
	}
	return pluginsReadyPromise;
};

const resolvePluginsReady = () => {
	pluginsReadyDone = true;
	markPluginsReady?.();
	markPluginsReady = null;
};

const focusMainWindow = async () => {
	const { getCurrentWindow } = await import('@tauri-apps/api/window');
	const win = getCurrentWindow();
	await win.show();
	await win.setFocus();
};

/** The plugin never learns anything about the click itself; the host already ran the command, if any. */
const followNotificationClick = async (
	host: ReturnType<typeof createPluginHost>,
	store: Store<any>,
	token: string,
) => {
	const result = await host.resolveNotificationClick(token).catch(() => null);
	if (
		result?.type === 'task' &&
		result.taskId != null &&
		result.pluginId &&
		host.isCurrentRun(result.pluginId, {
			generation: result.generation ?? null,
			workspaceId: result.workspaceId ?? null,
		})
	) {
		store.commit('setCurrentTaskIdForModal', result.taskId);
	}
};

/** A tray click may arrive while the app is hidden; only bring it forward for a task, not a silent command. */
const followTrayClick = async (
	host: ReturnType<typeof createPluginHost>,
	store: Store<any>,
	id: string,
) => {
	const result = await host.resolveTrayClick(id).catch(() => null);
	if (result?.type === 'task' && result.taskId != null) {
		await focusMainWindow();
		if (
			!result.pluginId ||
			!host.isCurrentRun(result.pluginId, {
				generation: result.generation ?? null,
				workspaceId: result.workspaceId ?? null,
			})
		)
			return;
		store.commit('setCurrentTaskIdForModal', result.taskId);
	}
};

const showPluginNotification = (
	host: ReturnType<typeof createPluginHost>,
	store: Store<any>,
	pluginName: string,
	payload: {
		message: string;
		title: string | null;
		command: string | null;
		urgency: 'normal' | 'high';
		token: string | null;
	},
) => {
	const title = payload.title ?? pluginName;
	if (document.hasFocus() && payload.urgency === 'normal') {
		toast({
			title,
			description: payload.message,
			action: payload.token
				? h(
						ToastAction,
						{
							altText: payload.command ? 'Run' : 'Open',
							onClick: () => void followNotificationClick(host, store, payload.token!),
						},
						() => (payload.command ? 'Run' : 'Open'),
				  )
				: undefined,
		});
		return;
	}
	void showNativeNotification(title, payload.message, payload.token);
};

// Not the web Notification API: tauri-plugin-notification's init script replaces it with a click-less shim.
const showNativeNotification = async (
	title: string,
	body: string,
	token: string | null,
) => {
	const { invoke } = await import('@tauri-apps/api/core');
	await invoke('plugin_notify', { title, body, token }).catch((error) =>
		console.error('plugin_notify failed', error),
	);
};

/** Desktop only: starts the plugin host and follows the current workspace. */
export const installPlugins = async (
	store: Store<any>,
	safeModeFlag: boolean,
) => {
	memberId = () => Number(store.state.user?.id ?? 0);
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
		api: (pluginId, workspace, storageId, pluginName) =>
			workspace.kind === 'cloud'
				? createDataApi(
						cloudClientFor(workspace.id, pluginId),
						pluginId,
						pluginId,
						pluginName,
						true,
						workspace.id,
				  )
				: createDataApi(
						clientFor(workspace.id, store),
						pluginId,
						storageId,
						pluginName,
						false,
						workspace.id,
				  ),
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
			hasMachineConsent(workspace, pluginId, memberId()),
		trayTitleOwner: () => trayTitlePluginStore.get(),
		settings: settingsStore,
		notify: (title, description) => toast({ title, description }),
		notifyPlugin: (_pluginId, pluginName, payload) =>
			showPluginNotification(host, store, pluginName, payload),
		dnd: () => ({
			active: isDndActive(),
			until: dndState.until == null ? null : new Date(dndState.until).toISOString(),
		}),
		alarms: {
			get: (key) => alarmsStore.get(key) as any,
			set: (key, defs) => alarmsStore.set(key, defs),
		},
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
		requestLocalConnection: (pluginId, opts) => {
			const pending = requestLocalAccessConnect({
				pluginId,
				pluginName: pluginState.plugins[pluginId]?.manifest.name ?? pluginId,
				label: opts.label,
				permissions: opts.permissions,
			});
			void import('@/router')
				.then(({ default: router }) => router.push({ name: 'PersonaSettings' }))
				.catch(() => undefined);
			return pending;
		},
		revokePluginTokens: async (pluginId) => {
			const { invoke } = await import('@tauri-apps/api/core');
			await invoke('local_token_revoke_all', { pluginId }).catch(() => undefined);
		},
		// Not the app's own origin: the main window's on_new_window hook denies the popup and hands the
		// URL to the system opener, the same way any other external link in the app already opens.
		openExternal: (url) => {
			window.open(url, '_blank', 'noopener,noreferrer');
		},
		currentWorkspaceId: () => {
			const id = Number(store.getters.currentWorkspaceId);
			return Number.isFinite(id) && id !== 0 ? id : null;
		},
		blocked: (pluginId) =>
			installedBlocked.get(pluginId) ?? blockedById(pluginCatalog, pluginId),
	});
	setPluginHost(host);
	devPluginWatcher = startDevPluginWatcher(host);
	const { invoke } = await import('@tauri-apps/api/core');
	setCatalog(
		await invoke<Catalog>('plugin_catalog').catch(() => pluginCatalog),
	);
	await answerPluginWindows(host);
	startDndClock();
	syncDndToTray();
	const wake = () => {
		// The webview's own setInterval is throttled while hidden, so an expired DND must be caught here too.
		expireDndIfNeeded();
		host.tick();
	};
	document.addEventListener('visibilitychange', () => {
		if (document.visibilityState === 'visible') wake();
	});
	window.addEventListener('focus', wake);
	const { listen } = await import('@tauri-apps/api/event');
	await listen('plugins://tick', wake);
	await listen<string>('plugins://notification-click', ({ payload }) =>
		void followNotificationClick(host, store, payload),
	);
	await listen<string>('tray://plugin-item', ({ payload }) =>
		void followTrayClick(host, store, payload),
	);
	// The relay is only a hint: the routine is read back from that local workspace, never taken from the payload.
	await listen<{ workspaceId?: unknown; routineId?: unknown }>(
		'quick-add://routine-created',
		({ payload }) =>
			void (async () => {
				const workspaceId = Number(payload?.workspaceId);
				const routineId = Number(payload?.routineId);
				if (!Number.isSafeInteger(workspaceId) || !Number.isSafeInteger(routineId)) return;
				if (!(await localWorkspaceById(workspaceId))) return;
				const { data } = await clientFor(workspaceId, store).get(`daily-routines/tasks/${routineId}`);
				if (data?.data) domainEvents.emit({ type: 'routine.created', workspaceId, routineId, routine: data.data });
			})().catch(() => undefined),
	);
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
	let session: string | null = null;
	watch(
		() =>
			`${store.getters.currentWorkspaceId}|${
				store.getters.currentWorkspace?.code ?? ''
			}|${store.state.token ? store.state.user?.id ?? '' : ''}`,
		async () => {
			const current = ++sequence;
			// Another person (or nobody) signed in: nothing of the previous session may keep running.
			const now = store.state.token ? String(store.state.user?.id ?? '') : '';
			const sessionChanged = now !== session;
			if (sessionChanged) {
				session = now;
				cloudClients.clear();
				Object.keys(workspacePlugins).forEach(
					(id) => delete workspacePlugins[Number(id)],
				);
			}
			// A throw here must still resolve pluginsReady(), not leave its callers waiting forever.
			try {
				if (!now) {
					await host.activate(null);
					return;
				}
				const workspace = await resolveWorkspace(store);
				if (current !== sequence) return;
				const active = pluginState.workspace;
				if (
					!sessionChanged &&
					active?.id === workspace?.id &&
					active?.code === workspace?.code
				)
					return;
				if (workspace?.kind === 'cloud') {
					await loadWorkspacePlugins(workspace.id).catch(() => undefined);
					await syncWorkspacePlugins(workspace.id);
					if (current !== sequence) return;
				}
				await host.activate(workspace);
			} finally {
				if (current === sequence) resolvePluginsReady();
			}
		},
		{ immediate: true },
	);
	let pageSubscription: { workspaceId: number; id: string } | null = null;
	watch(
		() => pluginState.workspace,
		(workspace) => {
			if (pageSubscription)
				usePusher().unsubscribeHandlerFromWorkspace(
					pageSubscription.workspaceId,
					pageSubscription.id,
				);
			pageSubscription = null;
			if (workspace?.kind !== 'cloud') return;
			pageSubscription = {
				workspaceId: workspace.id,
				id: usePusher().subscribeToWorkspace(workspace.id, {
					onPageEvent: (type, { page }) => {
						const actor =
							page?.updated_by?.kind === 'plugin'
								? `plugin:${page.updated_by.id}`
								: undefined;
						domainEvents.emit(
							type === 'page.deleted'
								? { type, workspaceId: workspace.id, pageId: page.id, actor }
								: { type, workspaceId: workspace.id, pageId: page.id, page, actor },
						);
					},
				}),
			};
		},
		{ immediate: true },
	);
};

/** The one plugin the user chose in Settings to show text in the menu bar, or none to clear it. */
export const setTrayTitlePlugin = (pluginId: string | null) => {
	trayTitlePluginStore.set(pluginId);
	pluginHost()?.refreshTrayTitleOwner();
};

/** Re-reads folder plugins (developer mode) and restarts everything for the current workspace. */
export const reloadPlugins = async () => {
	const host = pluginHost();
	if (!host) return;
	devPluginWatcher?.reset();
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
	const entry = pluginState.plugins[pluginId];
	const storageId = entry ? storageIdOf(entry) : pluginId;
	await invoke('plugin_uninstall', { id: pluginId });
	await invoke('local_token_revoke_all', { pluginId }).catch(() => undefined);
	forgetPlugin(pluginId, storageId);
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
	if (!record || memberId() <= 0) return;
	machineConsentStore.set(
		memberId(),
		workspace.id,
		pluginId,
		releaseOf(record),
		allowed,
	);
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
