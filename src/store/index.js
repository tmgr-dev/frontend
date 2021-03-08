import { createStore } from 'vuex'
import colorSchemes from '../colors/schemes'
import axios from 'axios'
import pusherBeamsClient from './plugins/pusher-beams-client'
import pusherTokenProvider from './plugins/pusher-token-provider'
import pusher from './plugins/pusher'
const color = (colorKey, colorScheme) => colorSchemes[colorScheme][colorKey]

const token = localStorage.getItem('token') ? JSON.parse(localStorage.getItem('token')) : null;

const state = {
	apiBaseUrl: process.env.VUE_APP_API_BASE_URL,
	token: token,
	currentOpenedTaskId: null,
	user: localStorage.getItem('user') ? JSON.parse(localStorage.getItem('user')) : null,
	colorScheme: localStorage.getItem('colorScheme') || 'default',
	pusherBeamsUserId: null,
	pusherBeamsClient: pusherBeamsClient,
	pusherTokenProvider: pusherTokenProvider(token),
	sideout: null,
	pusher: pusher(token),
	userSettings: {
		showTooltips: true
	}
};

const getters = {
	apiBaseUrl: state => state.apiBaseUrl,
	token: state => state.token,
	pusherBeamsUserId: state => state.pusherBeamsUserId,
	pusherBeamsClient: state => state.pusherBeamsClient,
	pusherTokenProvider: state => state.pusherTokenProvider,
	currentOpenedTaskId: state => state.currentOpenedTaskId,
	pusher: state => state.pusher,
	slideout: state => state.slideout,
	user: state => state.user,
	isLoggedIn: state => state.token !== null,
	colorScheme: state => state.colorScheme,
	getUserSettings: state => state.userSettings
};

const mutations = {
	token(state, token) {
		if (token == null) {
			localStorage.removeItem('token')
		} else {
			localStorage.setItem('token', JSON.stringify(token))
		}

		state.token = token
	},
	user(state, user) {
		if (user == null) {
			localStorage.removeItem('user')
		} else {
			localStorage.setItem('user', JSON.stringify(user))
		}

		state.user = user
	},
	pusherBeamsUserId(state, pusherBeamsUserId) {
		state.pusherBeamsUserId = pusherBeamsUserId
	},
	currentOpenedTaskId(state, currentOpenedTaskId) {
		state.currentOpenedTaskId = currentOpenedTaskId
	},
	colorScheme(state, colorScheme) {
		if (colorScheme == null) {
			localStorage.removeItem('colorScheme')
		} else {
			localStorage.setItem('colorScheme', colorScheme)
		}

		state.colorScheme = colorScheme
		document.querySelector('body').className = color('bgBody', colorScheme)
	},
	setUserSettings (state, settings) {
		state.userSettings = settings
	},
	setSlideout (state, slideout) {
		state.slideout = slideout
	},
	setPusherBeamsClient (state, pusherBeamsClient) {
		state.pusherBeamsClient = pusherBeamsClient
	}
}

const actions = {
	logout() {
		localStorage.clear()
		document.location.reload()
	},
	async loadUserSettings ({ commit, state }) {
		if (!state.user) {
			return
		}
		try {
			const { data: {data} } = await axios.get(`user/settings`)
			if (data instanceof Object && data.hasOwnProperty('settings')) {
				commit('setUserSettings', data.settings)
			}
		} catch (e) {
			throw e
		}
	},
	async putUserSettings ({ commit }, settings) {
		try {
			await axios.put(`user/settings`, settings)
			commit('setUserSettings', settings)
		} catch (e) {
			console.error(e)
			throw e
		}
	},
}

export default createStore({
	state,
	getters,
	mutations,
	actions
})
