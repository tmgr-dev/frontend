import alertPlugin from '@/alert';
import components from '@/globalComponents.ts';
import { mask, VueTheMask } from '@/plugins/VueTheMask';
import Selectable from '@/plugins/directives/selectable';
import router from '@/router';
import store from '@/store';
import {
	desktopPlatform,
	desktopWindowLabel,
	installAutoHideScrollbars,
	isDesktopApp,
} from '@/utils/desktop';
import { isDetachedWindowLabel } from '@/utils/taskWindow';
import { applyRelayedEvent, installWindowEventRelay } from '@/utils/windowEventRelay';
import { installLocalWorkspaces } from '@/local/install';
import { activeLocalWorkspace } from '@/local/runtime';
import { requestCache } from '@/utils/requestCache';
import { domainEvents, installDomainEvents } from '@/utils/domainEvents';
import $axios from '@/plugins/axios';
import { installDesktopDiagnostics } from '@/utils/desktopDiagnostics';
import { startUpdateChecks } from '@/utils/desktopUpdater';
import { installFileDropGuard } from '@/utils/fileDrag';
import { tokenFromStorageEvent } from '@/utils/tokenSync';
import '@fontsource/instrument-serif/400-italic.css';
import '@fontsource/instrument-serif/400.css';
import '@fontsource/exo-2/400.css';
import '@fontsource/exo-2/500.css';
import '@fontsource/exo-2/600.css';
import '@fontsource/exo-2/700.css';
import '@fontsource/jetbrains-mono/400.css';
import '@fontsource/jetbrains-mono/500.css';
import { createApp, watch } from 'vue';
import App from './App.vue';

if (isDesktopApp()) {
	document.documentElement.classList.add('tauri-desktop');
	const platform = desktopPlatform();
	if (platform) document.documentElement.classList.add(`tauri-${platform}`);
	installAutoHideScrollbars();
	installFileDropGuard(window);
	installLocalWorkspaces($axios, {
		currentUser: () => store.state.user,
		hasSession: () => !!store.state.token?.token,
	});
	installDomainEvents($axios, domainEvents, () => {
		const id = Number(store.getters.currentWorkspaceId);
		return Number.isFinite(id) && id !== 0 ? id : null;
	});
	const isMainWindow = desktopWindowLabel() === 'main';
	const isDetachedWindow = isDetachedWindowLabel(desktopWindowLabel());
	if (isMainWindow) startUpdateChecks();
	if (isMainWindow) {
		void Promise.all([
			import('@/pluginSystem/app'),
			import('@tauri-apps/api/core').then(({ invoke }) =>
				invoke<boolean>('plugins_safe_mode').catch(() => false),
			),
		]).then(([{ installPlugins }, safeMode]) => installPlugins(store, safeMode));
		void import('@/local/localAccess').then(({ installLocalAccess }) => installLocalAccess(store));
		void import('@/utils/quickAddPageRelay').then(({ installQuickAddPageHost }) =>
			installQuickAddPageHost(store),
		);
		void import('@/utils/taskWindowBridge').then(({ installTaskWindowHost }) =>
			installTaskWindowHost(router, store),
		);
		void Promise.all([
			import('@/pluginSystem/taskMenuRelay'),
			import('@/pluginSystem/taskMenuItems'),
			import('@/pluginSystem/state'),
		]).then(([relay, menu, state]) =>
			relay.installTaskMenuRelayHost(
				menu.currentTaskMenuItems,
				() => state.pluginState.workspace?.id ?? null,
				async (pluginId, command, taskId, workspaceId) => {
					const host = state.pluginHost();
					if (!host) throw new Error('Plugins are not ready');
					await host.runTaskMenuCommand(pluginId, command, taskId, workspaceId);
				},
				(onChange) =>
					watch(
						() =>
							JSON.stringify([
								state.pluginState.workspace?.id ?? null,
								menu.currentTaskMenuItems(),
							]),
						onChange,
					),
			),
		);
	}
	if (isDetachedWindow) {
		void import('@/pluginSystem/taskMenuRelay').then(({ installTaskMenuRelayClient }) =>
			installTaskMenuRelayClient(desktopWindowLabel() as string, () => {
				const id = Number(store.getters.currentWorkspaceId);
				return Number.isSafeInteger(id) && id > 0 ? id : null;
			}),
		);
	}
	if (isMainWindow || isDetachedWindow) {
		const label = desktopWindowLabel() as string;
		void import('@tauri-apps/api/event').then(({ emit, listen }) =>
			installWindowEventRelay(domainEvents, {
				label,
				emit,
				listen,
				onRelayed: (event) =>
					applyRelayedEvent(event, {
						invalidate: (key) => requestCache.invalidate(key),
						reloadActiveTasks: () => store.commit('incrementReloadActiveTasksKey'),
						deliverTimer: (timer) => {
							const userId = store.state.user?.id;
							if (!userId || timer.workspaceId == null || timer.workspaceId >= 0) return;
							if (activeLocalWorkspace()?.id !== timer.workspaceId) return;
							void import('@/composable/usePusher').then(({ deliverToUser }) =>
								deliverToUser(userId, (h) =>
									timer.type === 'timer.started'
										? h.onTaskCountdownStarted?.(timer.task)
										: h.onTaskCountdownStopped?.(timer.task),
								),
							);
						},
					}),
			}),
		);
		void Promise.all([
			import('@/local/liveUpdates'),
			import('@/composable/usePusher'),
			import('@/actions/tmgr/tasks'),
		]).then(([{ installLocalLiveUpdates }, { deliverToWorkspace }, { getTask }]) =>
			installLocalLiveUpdates({
				deliver: deliverToWorkspace,
				fetchTask: async (workspaceId, taskId) =>
					activeLocalWorkspace()?.id === workspaceId ? getTask(taskId) : null,
				invalidate: (key) => requestCache.invalidate(key),
			}),
		);
	}
}

store.commit('setColorScheme', localStorage.getItem('colorScheme'));
store.commit('setTheme', localStorage.getItem('theme') || 'default');

window
	.matchMedia?.('(prefers-color-scheme: dark)')
	.addEventListener?.('change', (event) => {
		store.commit('setSystemPrefersDark', event.matches);
	});

// Another tab rotated the token (or logged out): adopt it here so this tab
// never replays a retired refresh token. setToken re-persists the same value,
// which fires no storage event in this tab and a no-op one in the others.
window.addEventListener('storage', (event) => {
	const result = tokenFromStorageEvent(event, store.state.token);
	if (result.changed) {
		store.commit('setToken', result.token);
	}
});

const app = createApp(App);

components.map((component) => app.component(component.name || '', component));
app.component('VueTheMask', VueTheMask);
app.directive('mask', mask);
app.use(Selectable);
app.use(router);
app.use(store);

alertPlugin({ app });

if (isDesktopApp()) installDesktopDiagnostics(app, router, store);

app.mount('#app');

// Pause decorative skeleton animations when this document is not visible.
const updatePageVisibility = () => {
	document.documentElement.dataset.pageHidden = String(document.hidden);
};
updatePageVisibility();
document.addEventListener('visibilitychange', updatePageVisibility);
