import { createStore } from 'vuex';
import pusherModule from 'src/store/modules/pusher';

const token = localStorage.getItem('token')
	? JSON.parse(localStorage.getItem('token') || '')
	: null;

const state = {
	token: token,
	user: localStorage.getItem('user')
		? JSON.parse(localStorage.getItem('user') || '')
		: null,
	colorScheme: localStorage.getItem('colorScheme') || 'default',
	currentTaskIdForModal: null,
	createTaskInProjectCategoryId: null,
	taskStatusId: null,
	showCreatingTaskModal: false,
	reloadTasksKey: 0,
	appRerenderKey: 0,
	workspaceStatuses: [],
	userSettings: {
		showTooltips: true,
	},
};

const getters = {
	isLoggedIn: (state) => state.token !== null,
};

const mutations = {
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
		if (user == null) {
			localStorage.removeItem('user');
		} else {
			localStorage.setItem('user', JSON.stringify(user));
		}

		state.user = user;
	},
	incrementReloadTasksKey(state) {
		state.reloadTasksKey++;
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
};

const actions = {
	logout() {
		const theme = localStorage.getItem('colorScheme');
		localStorage.clear();
		localStorage.setItem('colorScheme', theme);

		location.reload();
	},
};

const modules = {
	pusher: pusherModule,
};

export default createStore({
	state,
	getters,
	mutations,
	actions,
	modules,
});
