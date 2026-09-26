<template>
	<span class="hidden" />
</template>

<script>
	import {
		startTaskTimeCounter,
		stopTaskTimeCounter,
	} from '@/actions/tmgr/tasks';
	import store from '@/store';
	import {
		parseDeepLink,
		SHORTCUT_ACTIONS,
		shortcutConfig,
		shortcutStatus,
	} from '@/utils/desktopShortcuts';
	import { loadRecent } from '@/utils/desktopTray';
	import { defineComponent, onBeforeUnmount, onMounted, watch } from 'vue';

	const showMainWindow = async () => {
		const { getCurrentWindow } = await import('@tauri-apps/api/window');
		const win = getCurrentWindow();
		await win.show();
		await win.setFocus();
	};

	const openQuickAdd = async (extra = {}) => {
		const { invoke } = await import('@tauri-apps/api/core');
		await invoke('open_quick_add', {
			payload: { workspaceId: store.getters.currentWorkspaceId, ...extra },
		});
	};

	export default defineComponent({
		name: 'DesktopHotkeys',
		props: {
			tasks: { type: Array, default: () => [] },
		},
		setup(props) {
			let registered = [];
			let unlistenDeepLink = null;

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

			const openLinks = async (urls) => {
				const link = (urls || []).map(parseDeepLink).find(Boolean);
				if (!link) return;
				await showMainWindow();
				store.commit('setCurrentTaskIdForModal', link.taskId);
			};

			watch(shortcutConfig, register, { deep: true });

			onMounted(async () => {
				await register(shortcutConfig.value);
				const deepLink = await import('@tauri-apps/plugin-deep-link');
				unlistenDeepLink = await deepLink.onOpenUrl(openLinks);
				openLinks(await deepLink.getCurrent());
			});

			onBeforeUnmount(async () => {
				unlistenDeepLink?.();
				if (registered.length) {
					const shortcuts = await import('@tauri-apps/plugin-global-shortcut');
					await shortcuts.unregister(registered).catch(() => {});
				}
			});

			return {};
		},
	});
</script>
