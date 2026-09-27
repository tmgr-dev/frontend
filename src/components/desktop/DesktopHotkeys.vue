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
		AlertDialogFooter,
		AlertDialogHeader,
		AlertDialogTitle,
	} from '@/components/ui/alert-dialog';
	import { Button } from '@/components/ui/button';
	import { toast } from '@/components/ui/toast';
	import { pluginsReady } from '@/pluginSystem/app';
	import { pluginHost, pluginState } from '@/pluginSystem/state';
	import { deepLinkConsentStore } from '@/pluginSystem/storage';
	import router from '@/router';
	import store from '@/store';
	import {
		parseDeepLink,
		SHORTCUT_ACTIONS,
		shortcutConfig,
		shortcutStatus,
	} from '@/utils/desktopShortcuts';
	import { loadRecent } from '@/utils/desktopTray';
	import { defineComponent, onBeforeUnmount, onMounted, ref, watch } from 'vue';

	const DEEP_LINK_TIMEOUT_MS = 10_000;

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
				if (registered.length) {
					await shortcuts.unregister(registered).catch(() => {});
				}
				registered = [];
				const status = {};
				for (const action of SHORTCUT_ACTIONS) {
					const { accelerator, enabled } = config[action];
					if (!enabled) {
						status[action] = 'off';
						continue;
					}
					try {
						await shortcuts.register(accelerator, (event) => {
							if (event.state !== 'Pressed') return;
							Promise.resolve(handlers[action]()).catch((error) =>
								console.error(`shortcut ${action} failed`, error),
							);
						});
						registered.push(accelerator);
						status[action] = 'ok';
					} catch (error) {
						console.error(`shortcut ${accelerator} not registered`, error);
						status[action] = 'taken';
					}
				}
				shortcutStatus.value = status;
			};

			const openViewLink = async (link) => {
				const view = pluginHost()?.deepLinkView(link.pluginId, link.viewId);
				if (!view) return cantOpenLink();
				await showMainWindow();
				if (view.ui) {
					await pluginHost()?.openView(link.pluginId, link.viewId, link.params);
					return;
				}
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

			/** Runs the command after re-checking it, in case the plugin stopped or left while the dialog was open. */
			const runCommandLink = async (link) => {
				try {
					const ran = await pluginHost()?.runDeepLinkCommand(
						link.pluginId,
						link.commandId,
						link.params,
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
				const link = pendingConfirm.value?.link;
				pendingConfirm.value = null;
				if (link) await runCommandLink(link);
			};

			const confirmAlways = async () => {
				const confirm = pendingConfirm.value;
				pendingConfirm.value = null;
				if (!confirm) return;
				const entry = pluginState.plugins[confirm.link.pluginId];
				if (entry) {
					deepLinkConsentStore.remember(
						confirm.link.pluginId,
						confirm.link.commandId,
						entry.manifest.version,
					);
				}
				await runCommandLink(confirm.link);
			};

			const openCommandLink = async (link) => {
				const command = pluginHost()?.deepLinkCommand(link.pluginId, link.commandId);
				if (!command) return cantOpenLink();
				const entry = pluginState.plugins[link.pluginId];
				const alwaysAllowed =
					!!entry &&
					deepLinkConsentStore.has(link.pluginId, link.commandId, entry.manifest.version);
				if (alwaysAllowed) return runCommandLink(link);
				await showMainWindow();
				pendingConfirm.value = {
					link,
					pluginName: entry?.manifest.name ?? link.pluginId,
					commandTitle: command.title,
				};
			};

			const openLinks = async (urls) => {
				const link = (urls || []).map(parseDeepLink).find(Boolean);
				if (!link) return;
				if (link.type === 'task') {
					await showMainWindow();
					store.commit('setCurrentTaskIdForModal', link.taskId);
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

			return { pendingConfirm, cancelConfirm, confirmOnce, confirmAlways };
		},
	});
</script>
