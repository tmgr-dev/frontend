import { createStore } from 'vuex';
import pusherModule from 'src/store/modules/pusher';

const token = localStorage.getItem('token')
	? JSON.parse(localStorage.getItem('token') || '')
	: null;

const state = {
	token: token,
	currentOpenedTaskId: null,
	user: localStorage.getItem('user')
		? JSON.parse(localStorage.getItem('user') || '')
		: null,
	colorScheme: localStorage.getItem('colorScheme') || 'default',
	currentTaskIdForModal: null,
	createTaskInProjectCategoryId: null,
	createTaskInStatusId: null,
	showCreateTaskModal: false,
	reloadTasksKey: null,
	appRerenderKey: 0,
	statuses: [],
	userSettings: {
		showTooltips: true,
	},
};

const getters = {
	getStatuses: (state) => state.statuses,
	getCurrentTaskIdForModal: (state) => state.currentTaskIdForModal,
	createTaskInProjectCategoryId: (state) => state.createTaskInProjectCategoryId,
	createTaskInStatusId: (state) => state.createTaskInStatusId,
	showCreateTaskModal: (state) => state.showCreateTaskModal,
	getCurrentOpenedTaskId: (state) => state.currentOpenedTaskId,
	isLoggedIn: (state) => state.token !== null,
	getColorScheme: (state) => state.colorScheme,
	getUserSettings: (state) => state.userSettings,
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
	setStatuses(state, data) {
		state.statuses = data;
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
	incrementReloadTasksKey({ state }) {
		if (!state.reloadTasksKey) {
			state.reloadTasksKey = 1;
		}

		++state.reloadTasksKey;
	},
	setCurrentTaskIdForModal(state, taskId) {
		state.currentTaskIdForModal = taskId;
	},
	showCreateTaskModal(state, statusId) {
		state.showCreateTaskModal = true;
		state.createTaskInStatusId = statusId;
	},
	createTaskInProjectCategoryId(state, { projectCategoryId, statusId }) {
		state.createTaskInStatusId = statusId;
		state.currentTaskIdForModal = null;
		state.createTaskInProjectCategoryId = projectCategoryId;
		state.showCreateTaskModal = true;
	},
	setCurrentOpenedTaskId(state, currentOpenedTaskId) {
		state.currentOpenedTaskId = currentOpenedTaskId;
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
		state.showCreateTaskModal = false;
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
