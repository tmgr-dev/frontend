import { getWorkspaces } from '@/actions/tmgr/workspaces';
import { disconnectRealtime } from '@/composable/usePusher';
import { clearPersonaLlmForLogout } from '@/local/personaCache';
import filterModule from '@/store/modules/boardFilters';
import dailyRoutinesModule from '@/store/modules/dailyRoutines';
import featureTogglesModule from '@/store/modules/featureToggles';
import pusherModule from '@/store/modules/pusher';
import { applyThemeToDocument, isDarkTheme } from '@/theme/applyTheme';
import { isDesktopApp } from '@/utils/desktop';
import { requestCache } from '@/utils/requestCache';
import {
	isKnownWorkspaceId,
	overlayCurrentWorkspace,
	readWorkspaceId,
	resolveWorkspaceId,
	WORKSPACE_LOCAL_KEY,
	WORKSPACE_SESSION_KEY,
	writeWorkspaceId,
} from '@/utils/workspaceContext';
import { createStore } from 'vuex';

const sessionStorageSafe = () => {
	try {
		return sessionStorage;
	} catch {
		return null;
	}
};
const localStorageSafe = () => {
	try {
		return localStorage;
	} catch {
		return null;
	}
};

const rememberClientWorkspaceId = (workspaceId) => {
	writeWorkspaceId(sessionStorageSafe(), WORKSPACE_SESSION_KEY, workspaceId);
	if (workspaceId != null) {
		writeWorkspaceId(localStorageSafe(), WORKSPACE_LOCAL_KEY, workspaceId);
	}
};

const token = localStorage.getItem('token')
	? JSON.parse(localStorage.getItem('token') || '')
	: null;

const invalidateWorkspaceScopedCache = () => {
	requestCache.invalidate('categories');
	requestCache.invalidate('statuses');
	requestCache.invalidate('workspace-statuses');
	requestCache.invalidate(/^tasks-status-/);
	requestCache.clearInFlight();
};

const state = {
	metaTitle: '',
	token: token,
	// A refresh rotates credentials without replacing the authenticated session.
	sessionGeneration: 0,
	/** @type {import('@/types/store').User | Record<string, never>} */
	user: {},
	colorScheme: localStorage.getItem('colorScheme') || 'system',
	systemPrefersDark:
		typeof window !== 'undefined' &&
		!!window.matchMedia &&
		window.matchMedia('(prefers-color-scheme: dark)').matches,
	theme: localStorage.getItem('theme') || 'default',
	currentTaskIdForModal: null,
	createTaskInProjectCategoryId: null,
	taskStatusId: null,
	showCreatingTaskModal: false,
	reloadActiveTasksKey: 0,
	reloadTasksKey: 0,
	createdTaskKey: 0,
	createdTaskData: null,
	deletedTaskKey: 0,
	deletedTaskId: null,
	appRerenderKey: 0,
	workspaceStatuses: [],
	workspaceStatusesById: {},
	workspaces: [],
	workspacesById: {},
	// The account's real current_workspace setting (server truth), never a per-tab override.
	defaultWorkspaceId: null,
	// This tab's chosen workspace (URL / sessionStorage / localStorage); overlaid onto
	// userSettingsMap['current_workspace'] so every existing reader sees it for free.
	clientWorkspaceId: null,
	userSettingsMap: {},
	userSettings: {
		showTooltips: true,
	},
	openModals: 0,
	modalStack: [],
	urlManuallyChanged: false,
	updatedTaskData: null,
	updatedTaskKey: 0,
	aiPanelOpen: false,
};

const getters = {
	isLoggedIn: (state) => state.token !== null,
	workspaceById: (state) => (id) => state.workspacesById[id],
	workspaceStatusById: (state) => (id) => state.workspaceStatusesById[id],
	userSettingByKey: (state) => (key) => state.userSettingsMap[key],
	currentWorkspaceId: (state) => {
		return state.userSettingsMap['current_workspace']?.value || null;
	},
	defaultWorkspaceId: (state) => state.defaultWorkspaceId,
	currentWorkspace: (state, getters) => {
		const workspaceId = getters.currentWorkspaceId;
		return workspaceId ? state.workspacesById[workspaceId] : null;
	},
	isDarkTheme: (state) =>
		isDarkTheme(state.theme, state.colorScheme, state.systemPrefersDark),
};

const mutations = {
	setMetaTitle(state, title) {
		state.metaTitle = title;
	},
	setWorkspaceStatuses(state, data) {
		if (Array.isArray(data)) {
			state.workspaceStatuses = data;
			state.workspaceStatusesById = data.reduce((acc, status) => {
				if (status && status.id) {
					acc[status.id] = status;
				}
				return acc;
			}, {});
		} else {
			state.workspaceStatuses = Object.values(data);
			state.workspaceStatusesById = data;
		}
	},
	setWorkspaces(state, workspaces) {
		if (Array.isArray(workspaces)) {
			state.workspaces = workspaces;
			state.workspacesById = workspaces.reduce((acc, workspace) => {
				if (workspace && workspace.id) {
					acc[workspace.id] = workspace;
				}
				return acc;
			}, {});
		} else {
			state.workspaces = Object.values(workspaces);
			state.workspacesById = workspaces;
		}

		// The list wasn't loaded yet when setUser first resolved the tab's workspace, so a
		// stale/foreign id trusted provisionally back then must be re-checked now.
		if (
			state.clientWorkspaceId != null &&
			!isKnownWorkspaceId(state.clientWorkspaceId, state.workspaces)
		) {
			state.clientWorkspaceId = resolveWorkspaceId({
				sessionWorkspaceId: readWorkspaceId(
					sessionStorageSafe(),
					WORKSPACE_SESSION_KEY,
				),
				lastWorkspaceId: readWorkspaceId(localStorageSafe(), WORKSPACE_LOCAL_KEY),
				defaultWorkspaceId: state.defaultWorkspaceId,
				workspaces: state.workspaces,
			});
			rememberClientWorkspaceId(state.clientWorkspaceId);
			if (Array.isArray(state.user?.settings)) {
				state.user.settings = overlayCurrentWorkspace(
					state.user.settings,
					state.clientWorkspaceId ?? state.defaultWorkspaceId,
				);
				state.userSettingsMap = state.user.settings.reduce((acc, setting) => {
					if (setting?.key) acc[setting.key] = setting;
					return acc;
				}, {});
			}
			requestCache.setContext(
				`${state.user?.id || 'guest'}:${state.clientWorkspaceId ?? ''}`,
			);
			invalidateWorkspaceScopedCache();
		}
	},
	updateSingleTask(state, task) {
		state.updatedTaskData = task;
		state.updatedTaskKey = (state.updatedTaskKey || 0) + 1;
	},
	// A task was created / deleted locally: the board and the list patch that one card
	// instead of reloading everything (TM-202).
	taskCreated(state, task) {
		state.createdTaskData = task;
		state.createdTaskKey = (state.createdTaskKey || 0) + 1;
	},
	taskDeleted(state, taskId) {
		state.deletedTaskId = taskId;
		state.deletedTaskKey = (state.deletedTaskKey || 0) + 1;
	},
	setToken(state, token) {
		if ((state.token == null) !== (token == null)) state.sessionGeneration++;
		if (token == null) {
			disconnectRealtime();
			requestCache.setContext('guest');
			requestCache.clear();
			localStorage.removeItem('token');
		} else {
			localStorage.setItem('token', JSON.stringify(token));
		}

		state.token = token;
	},
	setUser(state, user) {
		if (state.user?.id !== user?.id) {
			state.sessionGeneration++;
			requestCache.clear();
			if (state.dailyRoutines)
				dailyRoutinesModule.mutations.reset(state.dailyRoutines);
			// A token swap via the cross-tab storage listener skips the logout action (see
			// main.ts), so this tab's prior account's workspace choice must not leak to the
			// next one signed in here.
			state.defaultWorkspaceId = null;
			state.clientWorkspaceId = null;
		}
		const previousWorkspaceId =
			state.userSettingsMap['current_workspace']?.value || null;
		const nextUser = { ...user };
		if (Array.isArray(nextUser.settings)) {
			const previousSettingsById = Array.isArray(state.user?.settings)
				? state.user.settings.reduce((acc, setting) => {
						if (setting?.id) {
							acc[setting.id] = setting;
						}
						return acc;
				  }, {})
				: {};

			nextUser.settings = nextUser.settings.map((setting) => {
				if (setting?.key || !setting?.id || !previousSettingsById[setting.id]) {
					return setting;
				}
				return {
					...previousSettingsById[setting.id],
					...setting,
				};
			});

			// The account default is never negative — install.ts's own desktop overlay can hand
			// this mutation a local workspace's id while one is active, and that must not
			// corrupt the known default (state.clientWorkspaceId already tracks it separately).
			const rawWorkspaceSetting = nextUser.settings.find(
				(setting) => setting?.key === 'current_workspace',
			);
			const rawWorkspaceId =
				rawWorkspaceSetting?.value != null ? Number(rawWorkspaceSetting.value) : null;
			if (rawWorkspaceId != null && rawWorkspaceId >= 0) {
				state.defaultWorkspaceId = rawWorkspaceId;
			}

			if (state.clientWorkspaceId == null) {
				state.clientWorkspaceId = resolveWorkspaceId({
					sessionWorkspaceId: readWorkspaceId(
						sessionStorageSafe(),
						WORKSPACE_SESSION_KEY,
					),
					lastWorkspaceId: readWorkspaceId(localStorageSafe(), WORKSPACE_LOCAL_KEY),
					defaultWorkspaceId: state.defaultWorkspaceId,
					workspaces: state.workspaces,
				});
			}

			nextUser.settings = overlayCurrentWorkspace(
				nextUser.settings,
				state.clientWorkspaceId ?? state.defaultWorkspaceId,
			);
		}

		state.user = nextUser;
		if (user && user.settings && Array.isArray(user.settings)) {
			state.userSettingsMap = state.user.settings.reduce((acc, setting) => {
				if (setting?.key) {
					acc[setting.key] = setting;
				}
				return acc;
			}, {});
		}

		const nextWorkspaceId =
			state.userSettingsMap['current_workspace']?.value || null;
		requestCache.setContext(
			`${state.user?.id || 'guest'}:${nextWorkspaceId || ''}`,
		);
		if (
			previousWorkspaceId &&
			nextWorkspaceId &&
			Number(previousWorkspaceId) !== Number(nextWorkspaceId)
		) {
			invalidateWorkspaceScopedCache();
		}
	},
	incrementReloadTasksKey(state) {
		state.reloadTasksKey++;
	},
	incrementReloadActiveTasksKey(state) {
		state.reloadActiveTasksKey++;
	},
	setCurrentTaskIdForModal(state, taskId) {
		state.currentTaskIdForModal = taskId;
	},
	setShowCreatingTaskModal(state, statusId) {
		state.showCreatingTaskModal = true;
		state.taskStatusId = statusId;
	},
	createTaskInProjectCategoryId(state, { projectCategoryId, statusId }) {
		state.taskStatusId = statusId;
		state.currentTaskIdForModal = null;
		state.createTaskInProjectCategoryId = projectCategoryId;
		state.showCreatingTaskModal = true;
	},
	setColorScheme(state, colorScheme) {
		const normalized =
			colorScheme === 'dark'
				? 'dark'
				: colorScheme === 'system' || colorScheme == null
					? 'system'
					: 'default';
		state.userSettings.colorScheme = normalized;
		state.colorScheme = normalized;
		localStorage.setItem('colorScheme', normalized);
		applyThemeToDocument(state.theme, normalized);
	},
	setSystemPrefersDark(state, prefersDark) {
		state.systemPrefersDark = prefersDark;
		applyThemeToDocument(state.theme, state.colorScheme);
	},
	setTheme(state, theme) {
		const normalized = theme || 'default';
		state.theme = normalized;
		localStorage.setItem('theme', normalized);
		applyThemeToDocument(normalized, state.colorScheme);
	},
	setThemeToSystem(state) {
		state.theme = 'default';
		state.colorScheme = 'system';
		applyThemeToDocument('default', state.colorScheme);
	},
	closeTaskModal(state) {
		// Instead of using history.back() which can cause navigation issues,
		// simply reset state flags and let the modal close naturally
		state.urlManuallyChanged = false;
		state.currentTaskIdForModal = null;
		state.createTaskInProjectCategoryId = null;
		state.showCreatingTaskModal = false;
	},
	setUserSettings(state, settings) {
		state.userSettings = settings;
	},
	openModal(state) {
		state.openModals++;
	},
	closeModal(state) {
		state.openModals--;
	},
	resetOpenModals(state) {
		state.openModals = 0;
	},
	pushModalToStack(state, modalId) {
		if (!state.modalStack.includes(modalId)) {
			state.modalStack.push(modalId);
		}
	},
	removeModalFromStack(state, modalId) {
		const index = state.modalStack.indexOf(modalId);
		if (index > -1) {
			state.modalStack.splice(index, 1);
		}
	},
	clearModalStack(state) {
		state.modalStack = [];
	},
	rerenderApp(state) {
		state.appRerenderKey++;
	},
	// The single local-switch mechanism: this tab's workspace changes, nothing is sent to the
	// server. Callers that already PUT a genuine account-default change (the "Default workspace"
	// setting) go through setUser instead, which re-derives everything from the server response.
	updateUserWorkspaceSetting(state, { workspaceId }) {
		state.clientWorkspaceId = workspaceId != null ? Number(workspaceId) : null;
		rememberClientWorkspaceId(state.clientWorkspaceId);
		requestCache.setContext(
			`${state.user?.id || 'guest'}:${workspaceId || ''}`,
		);
		const previousWorkspaceId =
			state.userSettingsMap['current_workspace']?.value || null;
		if (state.user && state.user.settings) {
			const settingIndex = state.user.settings.findIndex(
				(s) => s.key === 'current_workspace',
			);
			if (settingIndex !== -1) {
				state.user.settings[settingIndex] = {
					...state.user.settings[settingIndex],
					value: workspaceId,
				};
			}
			if (state.userSettingsMap['current_workspace']) {
				state.userSettingsMap['current_workspace'] = {
					...state.userSettingsMap['current_workspace'],
					value: workspaceId,
				};
			}
		}
		if (
			previousWorkspaceId &&
			workspaceId &&
			Number(previousWorkspaceId) !== Number(workspaceId)
		) {
			invalidateWorkspaceScopedCache();
		}
	},
	setAiPanelOpen(state, value) {
		state.aiPanelOpen = !!value;
	},
	toggleAiPanel(state) {
		state.aiPanelOpen = !state.aiPanelOpen;
	},
};

const actions = {
	logout({ commit, state }) {
		// TEMP diagnostics: who triggers logout on hard reload (remove once found)
		console.warn('[auth] logout called', new Error().stack);
		// Only the session dies: UI prefs (colorScheme, sidebarExpanded,
		// preferred_editor, …) survive, but per-user data must not leak to
		// the next account on a shared browser.
		const userId = state.user?.id;
		if (isDesktopApp() && userId != null) {
			clearPersonaLlmForLogout(userId).catch((error) => {
				console.error('[auth] failed to clear persona LLM data on logout', error);
			});
		}
		disconnectRealtime();
		commit('setToken', null);
		commit('dailyRoutines/reset');
		commit('setAiPanelOpen', false);
		localStorage.removeItem('newTaskWithCheckpoints');
		Object.keys(localStorage)
			.filter((key) => key.startsWith('pomo-enabled-'))
			.forEach((key) => localStorage.removeItem(key));
		localStorage.removeItem('theme');
		localStorage.removeItem('colorScheme');
		commit('setThemeToSystem');
		writeWorkspaceId(sessionStorageSafe(), WORKSPACE_SESSION_KEY, null);
		writeWorkspaceId(localStorageSafe(), WORKSPACE_LOCAL_KEY, null);
		state.defaultWorkspaceId = null;
		state.clientWorkspaceId = null;
		requestCache.clear();
	},

	async loadWorkspaces({ commit, state }) {
		const sessionGeneration = state.sessionGeneration;
		const userId = state.user?.id;
		try {
			const workspaces = await getWorkspaces();
			if (
				state.sessionGeneration === sessionGeneration &&
				state.user?.id === userId
			)
				commit('setWorkspaces', workspaces);
			return workspaces;
		} catch (error) {
			console.error('Error loading workspaces:', error);
			return [];
		}
	},
};

const modules = {
	pusher: pusherModule,
	filter: filterModule,
	featureToggles: featureTogglesModule,
	dailyRoutines: dailyRoutinesModule,
};

export default createStore({
	state,
	getters,
	mutations,
	actions,
	modules,
});
