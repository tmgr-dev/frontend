import { createStore } from 'vuex';
import axios from 'axios';
import pusherBeamsClient from './plugins/pusher-beams-client';
import pusherTokenProvider from './plugins/pusher-token-provider';
import pusher from './plugins/pusher';

const token = localStorage.getItem('token')
	? JSON.parse(localStorage.getItem('token'))
	: null;

const state = {
	token: token,
	currentOpenedTaskId: null,
	user: localStorage.getItem('user')
		? JSON.parse(localStorage.getItem('user'))
		: null,
	colorScheme: localStorage.getItem('colorScheme') || 'default',
	pusherBeamsUserId: null,
	pusherBeamsClient: pusherBeamsClient,
	currentTaskIdForModal: null,
	createTaskInProjectCategoryId: null,
	createTaskInStatusId: null,
	showCreateTaskModal: false,
	reloadTasks: null,
	pusherTokenProvider: pusherTokenProvider(token),
	sideout: null,
	appRerender: 0,
	statuses: [],
	pusher: pusher(token),
	userSettings: {
		showTooltips: true,
	},
};

const getters = {
	token: (state) => state.token,
	appRerender: (state) => state.appRerender,
	statuses: (state) => state.statuses,
	reloadTasks: (state) => state.reloadTasks,
	currentTaskIdForModal: (state) => state.currentTaskIdForModal,
	createTaskInProjectCategoryId: (state) => state.createTaskInProjectCategoryId,
	createTaskInStatusId: (state) => state.createTaskInStatusId,
	showCreateTaskModal: (state) => state.showCreateTaskModal,
	pusherBeamsUserId: (state) => state.pusherBeamsUserId,
	pusherBeamsClient: (state) => state.pusherBeamsClient,
	pusherTokenProvider: (state) => state.pusherTokenProvider,
	currentOpenedTaskId: (state) => state.currentOpenedTaskId,
	pusher: (state) => state.pusher,
	slideout: (state) => state.slideout,
	user: (state) => state.user,
	isLoggedIn: (state) => state.token !== null,
	colorScheme: (state) => state.colorScheme,
	getUserSettings: (state) => state.userSettings,
};

const mutations = {
	token(state, token) {
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
	appRerender(state) {
		state.appRerender++;
		if (state.slideout?._opened) {
			console.log(state.slideout);
			state.slideout?.toggle();
		}
	},
	user(state, user) {
		if (user == null) {
			localStorage.removeItem('user');
		} else {
			localStorage.setItem('user', JSON.stringify(user));
		}

		state.user = user;
	},
	currentTaskIdForModal(state, taskId) {
		state.currentTaskIdForModal = taskId;
	},
	showCreateTaskModal(state, { showCreateTaskModal, statusId }) {
		state.showCreateTaskModal = showCreateTaskModal;
		state.createTaskInStatusId = statusId;
	},
	createTaskInProjectCategoryId(state, { projectCategoryId, statusId }) {
		state.createTaskInStatusId = statusId;
		state.currentTaskIdForModal = null;
		state.createTaskInProjectCategoryId = projectCategoryId;
		state.showCreateTaskModal = true;
	},
	pusherBeamsUserId(state, pusherBeamsUserId) {
		state.pusherBeamsUserId = pusherBeamsUserId;
	},
	currentOpenedTaskId(state, currentOpenedTaskId) {
		state.currentOpenedTaskId = currentOpenedTaskId;
	},
	colorScheme(state, colorScheme) {
		if (colorScheme) {
			state.userSettings.colorScheme = colorScheme;
		}
		state.colorScheme = colorScheme;
		localStorage.setItem('colorScheme', colorScheme);
		document.querySelector('html').className =
			colorScheme === 'dark' ? 'dark' : '';
	},
	setUserSettings(state, settings) {
		state.userSettings = settings;
	},
	setSlideout(state, slideout) {
		state.slideout = slideout;
	},
	setPusherBeamsClient(state, pusherBeamsClient) {
		state.pusherBeamsClient = pusherBeamsClient;
	},
};

const actions = {
	logout() {
		const theme = localStorage.getItem('colorScheme');
		localStorage.clear();
		localStorage.setItem('colorScheme', theme);

		location.reload();
	},
	reloadTasks({ state }) {
		if (!state.reloadTasks) {
			state.reloadTasks = 1;
		}
		++state.reloadTasks;
	},
	closeTaskModal({ state }) {
		state.currentTaskIdForModal = null;
		state.createTaskInProjectCategoryId = null;
		state.showCreateTaskModal = false;
	},
	showCreateTaskModal({ state }) {
		state.showCreateTaskModal = true;
	},
	async putUserSettings({ commit }, settings) {
		try {
			await axios.put(`user/settings`, { settings });

			commit('setUserSettings', settings);
		} catch (e) {
			console.error(e);
			throw e;
		}
	},
};

export default createStore({
	state,
	getters,
	mutations,
	actions,
});
