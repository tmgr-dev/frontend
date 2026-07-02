import filterModule from '@/store/modules/boardFilters';
import pusherModule from '@/store/modules/pusher';
import featureTogglesModule from '@/store/modules/featureToggles';
import dailyRoutinesModule from '@/store/modules/dailyRoutines';
import { createStore } from 'vuex';
import { getWorkspaces } from '@/actions/tmgr/workspaces';
import { requestCache } from '@/utils/requestCache';

const token = localStorage.getItem('token')
	? JSON.parse(localStorage.getItem('token') || '')
	: null;

const invalidateWorkspaceScopedCache = () => {
	requestCache.invalidate('categories');
	requestCache.invalidate('workspace-statuses');
	requestCache.invalidate(/^tasks-status-/);
	requestCache.clearInFlight();
};

const state = {
	metaTitle: '',
	token: token,
	user: {},
	colorScheme: localStorage.getItem('colorScheme') || 'default',
	currentTaskIdForModal: null,
	createTaskInProjectCategoryId: null,
	taskStatusId: null,
	showCreatingTaskModal: false,
	reloadActiveTasksKey: 0,
	reloadTasksKey: 0,
	appRerenderKey: 0,
	workspaceStatuses: [],
	workspaceStatusesById: {},
	workspaces: [],
	workspacesById: {},
	userSettingsMap: {},
	userSettings: {
		showTooltips: true,
	},
	openModals: 0,
	modalStack: [],
	urlManuallyChanged: false,
	updatedTaskData: null,
	updatedTaskKey: 0,
};

const getters = {
	isLoggedIn: (state) => state.token !== null,
	workspaceById: (state) => (id) => state.workspacesById[id],
	workspaceStatusById: (state) => (id) => state.workspaceStatusesById[id],
	userSettingByKey: (state) => (key) => state.userSettingsMap[key],
	currentWorkspaceId: (state) => {
		return state.userSettingsMap['current_workspace']?.value || null;
	},
	currentWorkspace: (state, getters) => {
		const workspaceId = getters.currentWorkspaceId;
		return workspaceId ? state.workspacesById[workspaceId] : null;
	},
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
	},
	updateSingleTask(state, task) {
		state.updatedTaskData = task;
		state.updatedTaskKey = (state.updatedTaskKey || 0) + 1;
	},
	setToken(state, token) {
		if (token == null) {
			localStorage.removeItem('token');
		} else {
			localStorage.setItem('token', JSON.stringify(token));
		}

		state.token = token;
	},
	setUser(state, user) {
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
		if (colorScheme) {
			state.userSettings.colorScheme = colorScheme;
		}
		state.colorScheme = colorScheme;
		localStorage.setItem('colorScheme', colorScheme);
		document.querySelector('html').className =
			colorScheme === 'dark' ? 'dark' : '';
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
	updateUserWorkspaceSetting(state, { workspaceId }) {
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
};

const actions = {
	logout({ commit }) {
		// Only the session dies: UI prefs (colorScheme, sidebarExpanded,
		// preferred_editor, …) survive, but per-user data must not leak to
		// the next account on a shared browser.
		commit('setToken', null);
		localStorage.removeItem('newTaskWithCheckpoints');
		Object.keys(localStorage)
			.filter((key) => key.startsWith('pomo-enabled-'))
			.forEach((key) => localStorage.removeItem(key));
		requestCache.clear();
	},

	async loadWorkspaces({ commit }) {
		try {
			const workspaces = await getWorkspaces();
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
