import alertPlugin from '@/alert';
import components from '@/globalComponents.ts';
import { mask, VueTheMask } from '@/plugins/VueTheMask';
import Selectable from '@/plugins/directives/selectable';
import router from '@/router';
import store from '@/store';
import {
	desktopWindowLabel,
	installAutoHideScrollbars,
	isDesktopApp,
} from '@/utils/desktop';
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
import { createApp } from 'vue';
import App from './App.vue';

if (isDesktopApp()) {
	document.documentElement.classList.add('tauri-desktop');
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
	if (desktopWindowLabel() === 'main') startUpdateChecks();
	if (desktopWindowLabel() === 'main') {
		void Promise.all([
			import('@/pluginSystem/app'),
			import('@tauri-apps/api/core').then(({ invoke }) =>
				invoke<boolean>('plugins_safe_mode').catch(() => false),
			),
		]).then(([{ installPlugins }, safeMode]) => installPlugins(store, safeMode));
		void import('@/local/localAccess').then(({ installLocalAccess }) => installLocalAccess(store));
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
