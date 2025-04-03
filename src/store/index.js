import filterModule from '@/store/modules/boardFilters';
import pusherModule from '@/store/modules/pusher';
import { createStore } from 'vuex';
import { getWorkspaces } from '@/actions/tmgr/workspaces';

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
	reloadActiveTasksKey: 0, // for the App component
	reloadTasksKey: 0, // for the board
	appRerenderKey: 0,
	workspaceStatuses: [],
	workspaces: [],
	userSettings: {
		showTooltips: true,
	},
	openModals: 0,
	urlManuallyChanged: false, // Track whether URL was manually changed by our code
};

const getters = {
	isLoggedIn: (state) => state.token !== null,
};

const mutations = {
	setMetaTitle(state, title) {
		state.metaTitle = title;
	},
	setWorkspaceStatuses(state, data) {
		state.workspaceStatuses = data;
	},
	setWorkspaces(state, workspaces) {
		state.workspaces = workspaces;
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
	rerenderApp(state) {
		state.appRerenderKey++;
	},
	updateUserWorkspaceSetting(state, { workspaceId }) {
		if (state.user && state.user.settings) {
			// Update the current_workspace setting
			state.user.settings = state.user.settings.map(setting => {
				if (setting.key === 'current_workspace') {
					return {
						...setting,
						value: workspaceId
					};
				}
				return setting;
			});
		}
	},
};

const actions = {
	logout() {
		const theme = localStorage.getItem('colorScheme');
		const workspaceInvitationToken = localStorage.getItem(
			'workspace.invitation',
		);
		localStorage.clear();
		localStorage.setItem('colorScheme', theme);
		if (workspaceInvitationToken) {
			localStorage.setItem('workspace.invitation', workspaceInvitationToken);
		}

		// location.reload();
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
};

export default createStore({
	state,
	getters,
	mutations,
	actions,
	modules,
});
