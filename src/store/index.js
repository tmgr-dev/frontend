import filterModule from '@/store/modules/boardFilters';
import pusherModule from '@/store/modules/pusher';
import featureTogglesModule from '@/store/modules/featureToggles';
import { createStore } from 'vuex';
import { getWorkspaces } from '@/actions/tmgr/workspaces';
import { requestCache } from '@/utils/requestCache';
import { requestDeduplicator } from '@/utils/requestDeduplicator';

const token = localStorage.getItem('token')
	? JSON.parse(localStorage.getItem('token') || '')
	: null;

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
		state.user = user;
		if (user && user.settings && Array.isArray(user.settings)) {
			state.userSettingsMap = user.settings.reduce((acc, setting) => {
				acc[setting.key] = setting;
				return acc;
			}, {});
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
		if (state.user && state.user.settings) {
			const settingIndex = state.user.settings.findIndex(
				s => s.key === 'current_workspace'
			);
			if (settingIndex !== -1) {
				state.user.settings[settingIndex] = {
					...state.user.settings[settingIndex],
					value: workspaceId
				};
			}
			if (state.userSettingsMap['current_workspace']) {
				state.userSettingsMap['current_workspace'] = {
					...state.userSettingsMap['current_workspace'],
					value: workspaceId
				};
			}
		}
	},
};

const actions = {
	logout() {
		const workspaceInvitationToken = localStorage.getItem(
			'workspace.invitation',
		);
		localStorage.clear();
		if (workspaceInvitationToken) {
			localStorage.setItem('workspace.invitation', workspaceInvitationToken);
		}
		
		requestCache.clear();
		requestDeduplicator.clear();
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
	}
};

const modules = {
	pusher: pusherModule,
	filter: filterModule,
	featureToggles: featureTogglesModule,
};

export default createStore({
	state,
	getters,
	mutations,
	actions,
	modules,
});
