import { toast } from '@/components/ui/toast';
import { usePusher } from '@/composable/usePusher';
import $axios from '@/plugins/axios';
import { domainEvents, installDomainEvents } from '@/utils/domainEvents';
import type { AxiosInstance } from 'axios';
import { watch } from 'vue';
import type { Store } from 'vuex';
import type { PluginWorkspace } from './broker';
import { builtinPackages } from './builtin';
import {
	BUILTIN_REPO,
	matchesPin,
	recordFor,
	workspacePlugins,
	type WorkspacePluginRecord,
} from './cloud';
import { cloudPluginClient, type MintedToken } from './cloudClient';
import { createDataApi } from './dataApi';
import { createPluginHost, type PluginPackage } from './host';
import { createLeader } from './leader';
import type { WorkerEndpoint } from './process';
import { installedPluginErrors, pluginState, setPluginHost } from './state';
import { settingsStore } from './storage';
import { packageFromPin, webRefusal } from './webSupport';

const workspaceOf = (workspace: any): PluginWorkspace | null =>
	workspace && Number(workspace.id) > 0
		? {
				id: Number(workspace.id),
				code: String(workspace.code),
				name: String(workspace.name),
				kind: 'cloud',
				ownerId:
					workspace.user_id == null ? undefined : Number(workspace.user_id),
			}
		: null;

const cloudClients = new Map<string, AxiosInstance>();

const cloudClientFor = (
	workspaceId: number,
	pluginId: string,
	sha256: string | undefined,
) => {
	const key = `${workspaceId}:${pluginId}:${sha256 ?? ''}`;
	let client = cloudClients.get(key);
	if (!client) {
		client = cloudPluginClient({
			baseURL: String($axios.defaults.baseURL ?? ''),
			workspaceId,
			mint: async () =>
				(
					await $axios.post<{ data: MintedToken }>(
						`workspaces/${workspaceId}/plugins/${pluginId}/token`,
						undefined,
						{ params: sha256 ? { sha256 } : undefined },
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

const downloadBundle = async (
	workspaceId: number,
	pluginId: string,
	sha256: string | null,
) =>
	(
		await $axios.get<ArrayBuffer>(
			`workspaces/${workspaceId}/plugins/${pluginId}/bundle`,
			{ responseType: 'arraybuffer', params: sha256 ? { sha256 } : undefined },
		)
	).data;

/** Plugins on the web: only what the creator of a cloud workspace pinned, checked against the pinned checksum. */
export const installWebPlugins = (store: Store<any>) => {
	installDomainEvents($axios, domainEvents, () => {
		const id = currentId();
		return Number.isFinite(id) && id !== 0 ? id : null;
	});
	const leader = createLeader();
	let host: ReturnType<typeof createPluginHost> | null = null;
	let pinned: { workspaceId: number; records: WorkspacePluginRecord[] } = {
		workspaceId: 0,
		records: [],
	};
	let requested = '';
	let sequence = 0;
	let session: string | null = null;
	let leading = false;
	let pageSubscription: { workspaceId: number; id: string } | null = null;

	const stopPageEvents = () => {
		if (pageSubscription)
			usePusher().unsubscribeHandlerFromWorkspace(
				pageSubscription.workspaceId,
				pageSubscription.id,
			);
		pageSubscription = null;
	};

	const startPageEvents = (workspaceId: number) => {
		stopPageEvents();
		pageSubscription = {
			workspaceId,
			id: usePusher().subscribeToWorkspace(workspaceId, {
				onPageEvent: (type, { page }) => {
					const actor =
						page?.updated_by?.kind === 'plugin'
							? `plugin:${page.updated_by.id}`
							: undefined;
					domainEvents.emit({
						type,
						workspaceId,
						pageId: page.id,
						page,
						actor,
					});
				},
			}),
		};
	};

	const stepDown = () => {
		leading = false;
		leader.leave();
		stopPageEvents();
	};

	const packages = async (): Promise<PluginPackage[]> => {
		const found: PluginPackage[] = [];
		const errors: Record<string, string> = {};
		const { workspaceId, records } = pinned;
		const loadedFor = sequence;
		for (const record of records) {
			try {
				if (record.repo === BUILTIN_REPO) {
					const builtin = builtinPackages().find(
						(pkg) => pkg.manifest.id === record.plugin_id,
					);
					const refusal = builtin ? webRefusal(builtin) : 'unknown built-in';
					if (!builtin || refusal) throw new Error(refusal ?? '');
					found.push(builtin);
				} else {
					found.push(
						await packageFromPin(record, () =>
							downloadBundle(workspaceId, record.plugin_id, record.sha256),
						),
					);
				}
			} catch (error) {
				errors[record.plugin_id] = `Shared workspace plugin: ${
					error instanceof Error ? error.message : String(error)
				}`;
			}
		}
		if (loadedFor === sequence) {
			Object.keys(installedPluginErrors).forEach(
				(key) => delete installedPluginErrors[key],
			);
			Object.assign(installedPluginErrors, errors);
		}
		return found;
	};

	const currentId = () => Number(store.getters.currentWorkspaceId);

	const ensureHost = () => {
		if (host) return host;
		host = createPluginHost({
			state: pluginState,
			packages,
			createEndpoint: () =>
				new Worker(new URL('./worker.ts', import.meta.url), {
					type: 'module',
				}) as unknown as WorkerEndpoint,
			api: (pluginId, workspace, _storageId, pluginName) => {
				if (workspace.kind !== 'cloud')
					throw new Error('plugins run in cloud workspaces on the web');
				return createDataApi(
					cloudClientFor(
						workspace.id,
						pluginId,
						pluginState.plugins[pluginId]?.origin?.sha256,
					),
					pluginId,
					pluginId,
					pluginName,
					true,
					workspace.id,
				);
			},
			subscribe: (handler) => domainEvents.on(handler),
			enabled: {
				get: (pluginId, workspaceId) => {
					const record = recordFor(workspaceId, pluginId);
					return !!record && matchesPin(record, pluginState.plugins[pluginId]);
				},
				set: () => undefined,
			},
			machineAllowed: () => false,
			alarms: { get: () => undefined, set: () => undefined },
			isLeader: () => leading,
			settings: settingsStore,
			notify: (title, description) => toast({ title, description }),
			notifyPlugin: (_pluginId, pluginName, payload) =>
				toast({
					title: payload.title ?? pluginName,
					description: payload.message,
				}),
			currentWorkspaceId: () => {
				const id = currentId();
				return Number.isFinite(id) && id !== 0 ? id : null;
			},
		});
		setPluginHost(host);
		return host;
	};

	watch(
		() =>
			`${store.getters.currentWorkspaceId}|${
				store.getters.currentWorkspace?.code ?? ''
			}|${store.state.token ? (store.state.user?.id ?? '') : ''}`,
		async () => {
			const user = store.state.token ? String(store.state.user?.id ?? '') : '';
			const workspace = user
				? workspaceOf(store.getters.currentWorkspace)
				: null;
			const target = `${user}|${workspace?.id ?? ''}|${workspace?.code ?? ''}`;
			if (target === requested) return;
			requested = target;
			const current = ++sequence;
			stepDown();
			if (user !== session) {
				session = user;
				cloudClients.clear();
				Object.keys(workspacePlugins).forEach(
					(id) => delete workspacePlugins[Number(id)],
				);
			}
			try {
				if (!workspace) {
					await host?.activate(null);
					return;
				}
				await loadWorkspacePlugins(workspace.id).catch(() => undefined);
				if (current !== sequence) return;
				pinned = {
					workspaceId: workspace.id,
					records: workspacePlugins[workspace.id] ?? [],
				};
				if (!pinned.records.length) {
					await host?.activate(null);
					return;
				}
				await ensureHost().load();
				if (current !== sequence) return;
				const promoted = () => {
					if (current !== sequence) return;
					leading = true;
					startPageEvents(workspace.id);
				};
				const now = await leader.lead(
					`tmgr-plugins:${user}:${workspace.id}`,
					promoted,
				);
				if (current !== sequence) return;
				if (now) promoted();
				await host!.activate(workspace);
			} catch (error) {
				console.error('[plugins] could not start on the web', error);
			}
		},
		{ immediate: true },
	);
};
