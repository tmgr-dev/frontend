<template>
	<span class="hidden" />
	<AlertDialog
		:open="!!pendingConfirm"
		@update:open="(open) => !open && cancelConfirm()"
	>
		<AlertDialogContent>
			<AlertDialogHeader>
				<AlertDialogTitle>
					Let {{ pendingConfirm?.pluginName }} run "{{
						pendingConfirm?.commandTitle
					}}" from a link?
				</AlertDialogTitle>
				<AlertDialogDescription v-if="pendingConfirmParams.length">
					<span
						v-for="param in pendingConfirmParams"
						:key="param.key"
						class="block truncate"
					>
						{{ param.key }}: {{ param.value }}
					</span>
				</AlertDialogDescription>
			</AlertDialogHeader>
			<AlertDialogFooter>
				<AlertDialogCancel @click="cancelConfirm">Cancel</AlertDialogCancel>
				<Button variant="outline" size="sm" @click="confirmOnce"
					>Allow once</Button
				>
				<AlertDialogAction @click="confirmAlways">Always</AlertDialogAction>
			</AlertDialogFooter>
		</AlertDialogContent>
	</AlertDialog>
</template>

<script>
	import {
		startTaskTimeCounter,
		stopTaskTimeCounter,
	} from '@/actions/tmgr/tasks';
	import {
		AlertDialog,
		AlertDialogAction,
		AlertDialogCancel,
		AlertDialogContent,
		AlertDialogDescription,
		AlertDialogFooter,
		AlertDialogHeader,
		AlertDialogTitle,
	} from '@/components/ui/alert-dialog';
	import { Button } from '@/components/ui/button';
	import { toast } from '@/components/ui/toast';
	import { getPage } from '@/actions/tmgr/pages';
	import { getWorkspaces } from '@/actions/tmgr/workspaces';
	import { pluginsReady } from '@/pluginSystem/app';
	import { storageIdOf } from '@/pluginSystem/host';
	import { pluginHost, pluginState } from '@/pluginSystem/state';
	import { deepLinkConsentStore } from '@/pluginSystem/storage';
	import router from '@/router';
	import {
		PLUGIN_WINDOWS_UNAVAILABLE,
		supportsPluginWindows,
	} from '@/utils/desktop';
	import store from '@/store';
	import {
		createRecentUrlGuard,
		parseDeepLink,
		registerShortcuts,
		shortcutConfig,
		shortcutStatus,
	} from '@/utils/desktopShortcuts';
	import { loadRecent } from '@/utils/desktopTray';
	import { openPageLink } from '@/utils/openPageLink';
	import { focusPageWindow } from '@/utils/pageWindow';
	import {
		computed,
		defineComponent,
		onBeforeUnmount,
		onMounted,
		ref,
		watch,
	} from 'vue';

	const DEEP_LINK_TIMEOUT_MS = 10_000;
	const MAX_PARAM_VALUE_DISPLAY = 60;

	const showMainWindow = async () => {
		const { getCurrentWindow } = await import('@tauri-apps/api/window');
		const win = getCurrentWindow();
		await win.show();
		await win.setFocus();
	};

	const cantOpenLink = () =>
		toast({ title: "Can't open this link", variant: 'destructive' });

	/** Module-scoped: a remount of this component must not run the cold-start link a second time. */
	let coldStartHandled = false;
	/** Module-scoped like `coldStartHandled`: getCurrent() and onOpenUrl may both deliver the same URL. */
	const isRecentDuplicateUrl = createRecentUrlGuard();

	const openQuickAdd = async (extra = {}) => {
		const { invoke } = await import('@tauri-apps/api/core');
		await invoke('open_quick_add', {
			payload: { workspaceId: store.getters.currentWorkspaceId, ...extra },
		});
	};

	export default defineComponent({
		name: 'DesktopHotkeys',
		components: {
			AlertDialog,
			AlertDialogAction,
			AlertDialogCancel,
			AlertDialogContent,
			AlertDialogDescription,
			AlertDialogFooter,
			AlertDialogHeader,
			AlertDialogTitle,
			Button,
		},
		props: {
			tasks: { type: Array, default: () => [] },
		},
		setup(props) {
			let registered = [];
			let unlistenDeepLink = null;
			const pendingConfirm = ref(null);

			const toggleTimer = async () => {
				if (props.tasks.length) {
					await Promise.all(
						props.tasks.map((task) =>
							stopTaskTimeCounter(task.id, task.workspace_id),
						),
					);
					return;
				}
				const last = loadRecent(store.state.user?.id)[0];
				if (last) await startTaskTimeCounter(last.id, last.workspaceId);
			};

			const captureScreenshot = async () => {
				const { invoke } = await import('@tauri-apps/api/core');
				const path = await invoke('capture_screenshot');
				if (path) await openQuickAdd({ screenshot: path });
			};

			const captureSelection = async () => {
				const { invoke } = await import('@tauri-apps/api/core');
				try {
					const text = await invoke('capture_selection');
					await openQuickAdd(text ? { text } : {});
				} catch (error) {
					await openQuickAdd({ error: String(error) });
				}
			};

			const handlers = {
				quickAdd: () => openQuickAdd(),
				timer: toggleTimer,
				screenshot: captureScreenshot,
				selection: captureSelection,
			};

			const register = async (config) => {
				const shortcuts = await import('@tauri-apps/plugin-global-shortcut');
				const result = await registerShortcuts(shortcuts, config, (action) => {
					Promise.resolve(handlers[action]()).catch((error) =>
						console.error(`shortcut ${action} failed`, error),
					);
				});
				registered = result.registered;
				shortcutStatus.value = result.status;
			};

			/** Captured right when a link is accepted, before any await: the run and workspace it is good for. */
			const capturedExpectation = (pluginId) => ({
				generation: pluginHost()?.generationOf(pluginId) ?? null,
				workspaceId: pluginHost()?.currentWorkspaceId() ?? null,
			});

			const openViewLink = async (link) => {
				const view = pluginHost()?.deepLinkView(link.pluginId, link.viewId);
				if (!view) return cantOpenLink();
				if (view.ui && !supportsPluginWindows()) {
					return toast({
						title: PLUGIN_WINDOWS_UNAVAILABLE,
						variant: 'destructive',
					});
				}
				const expected = capturedExpectation(link.pluginId);
				await showMainWindow();
				const result = await pluginHost()?.openDeepLinkView(
					link.pluginId,
					link.viewId,
					link.params,
					expected,
				);
				if (!result) return cantOpenLink();
				if (result.view.ui) return;
				await router.push({
					name: 'WorkspacePluginPage',
					params: {
						workspace_code: store.getters.currentWorkspace?.code,
						pluginId: link.pluginId,
						viewId: link.viewId,
					},
					query: link.params,
				});
			};

			/** Re-validates the run and workspace inside the host, right before it runs the command. */
			const runCommandLink = async (link, expected) => {
				try {
					const ran = await pluginHost()?.runDeepLinkCommand(
						link.pluginId,
						link.commandId,
						link.params,
						expected,
					);
					if (!ran) cantOpenLink();
				} catch (error) {
					console.error('deep link command failed', error);
				}
			};

			const cancelConfirm = () => {
				pendingConfirm.value = null;
			};

			const confirmOnce = async () => {
				const confirm = pendingConfirm.value;
				pendingConfirm.value = null;
				if (confirm) {
					await runCommandLink(confirm.link, {
						generation: confirm.generation,
						workspaceId: confirm.workspaceId,
					});
				}
			};

			const confirmAlways = async () => {
				const confirm = pendingConfirm.value;
				pendingConfirm.value = null;
				if (!confirm) return;
				deepLinkConsentStore.remember(confirm.storageId, confirm.link.commandId, confirm.version);
				await runCommandLink(confirm.link, {
					generation: confirm.generation,
					workspaceId: confirm.workspaceId,
				});
			};

			const openCommandLink = async (link) => {
				// A second command link while a dialog is open is ignored: not queued, not swapped in.
				if (pendingConfirm.value) return;
				const command = pluginHost()?.deepLinkCommand(link.pluginId, link.commandId);
				if (!command) return cantOpenLink();
				const entry = pluginState.plugins[link.pluginId];
				const storageId = entry ? storageIdOf(entry) : link.pluginId;
				const version = entry?.manifest.version ?? '';
				const expected = capturedExpectation(link.pluginId);
				if (deepLinkConsentStore.has(storageId, link.commandId, version)) {
					return runCommandLink(link, expected);
				}
				// Set before the await below: a link arriving during it must see the dialog already open.
				pendingConfirm.value = {
					link,
					pluginName: entry?.manifest.name ?? link.pluginId,
					commandTitle: command.title,
					storageId,
					version,
					generation: expected.generation,
					workspaceId: expected.workspaceId,
				};
				await showMainWindow();
			};

			const openLinks = async (urls) => {
				const url = (urls || []).find((u) => {
					const parsed = parseDeepLink(u);
					return parsed && parsed.type !== 'auth';
				});
				if (!url || isRecentDuplicateUrl(url)) return;
				const link = parseDeepLink(url);
				if (link.type === 'task') {
					await showMainWindow();
					store.commit('setCurrentTaskIdForModal', link.taskId);
					return;
				}
				if (link.type === 'page') {
					const opened = await openPageLink(link, {
						loadWorkspaces: async () => {
							if (!store.state.workspaces?.length) {
								store.commit('setWorkspaces', await getWorkspaces());
							}
							return store.state.workspaces || [];
						},
						currentWorkspaceId: () => store.getters.currentWorkspaceId,
						resolvePageId: async (slug) => (await getPage(slug)).id,
						focusPageWindow,
						showMain: showMainWindow,
						navigate: (path) => router.push(path),
					}).catch(() => false);
					if (!opened) cantOpenLink();
					return;
				}
				const ready = await Promise.race([
					pluginsReady().then(() => true),
					new Promise((resolve) => setTimeout(() => resolve(false), DEEP_LINK_TIMEOUT_MS)),
				]);
				if (!ready) return cantOpenLink();
				if (link.type === 'view') return openViewLink(link);
				return openCommandLink(link);
			};

			watch(shortcutConfig, register, { deep: true });

			onMounted(async () => {
				await register(shortcutConfig.value);
				const deepLink = await import('@tauri-apps/plugin-deep-link');
				unlistenDeepLink = await deepLink.onOpenUrl(openLinks);
				if (!coldStartHandled) {
					coldStartHandled = true;
					openLinks(await deepLink.getCurrent());
				}
			});

			onBeforeUnmount(async () => {
				unlistenDeepLink?.();
				if (registered.length) {
					const shortcuts = await import('@tauri-apps/plugin-global-shortcut');
					await shortcuts.unregister(registered).catch(() => {});
				}
			});

			const pendingConfirmParams = computed(() =>
				Object.entries(pendingConfirm.value?.link.params ?? {}).map(([key, value]) => ({
					key,
					value:
						value.length > MAX_PARAM_VALUE_DISPLAY
							? `${value.slice(0, MAX_PARAM_VALUE_DISPLAY)}…`
							: value,
				})),
			);

			return {
				pendingConfirm,
				pendingConfirmParams,
				cancelConfirm,
				confirmOnce,
				confirmAlways,
			};
		},
	});
</script>
