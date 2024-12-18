import filterModule from '@/store/modules/boardFilters';
import pusherModule from '@/store/modules/pusher';
import { createStore } from 'vuex';

const token = localStorage.getItem('token')
	? JSON.parse(localStorage.getItem('token') || '')
	: null;

const state = {
	metaTitle: '',
	token: token,
	user: localStorage.getItem('user')
		? JSON.parse(localStorage.getItem('user') || '')
		: null,
	colorScheme: localStorage.getItem('colorScheme') || 'default',
	currentTaskIdForModal: null,
	createTaskInProjectCategoryId: null,
	taskStatusId: null,
	showCreatingTaskModal: false,
	reloadActiveTasksKey: 0, // for the App component
	reloadTasksKey: 0, // for the board
	appRerenderKey: 0,
	workspaceStatuses: [],
	userSettings: {
		showTooltips: true,
	},
	openModals: 0,
};

const getters = {
	isLoggedIn: (state) => state.token !== null,
};

const mutations = {
	setMetaTitle(state, title) {
		state.metaTitle = title;
	},
	setToken(state, token) {
		if (token == null) {
			localStorage.removeItem('token');
		} else {
			localStorage.setItem('token', JSON.stringify(token));
		}

		state.token = token;
	},
	setWorkspaceStatuses(state, data) {
		state.workspaceStatuses = data;
	},
	rerenderApp(state) {
		state.appRerenderKey++;
	},
	setUser(state, user) {
		localStorage.setItem('user', JSON.stringify(user));

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

		location.reload();
	},
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
