import { createStore } from 'vuex'
import colorSchemes from '../colors/schemes'
import axios from 'axios'

const color = (colorKey, colorScheme) => colorSchemes[colorScheme][colorKey]

import * as PusherPushNotifications from "@pusher/push-notifications-web";
const token = localStorage.getItem('token') ? JSON.parse(localStorage.getItem('token')) : null;
const state = {
	apiBaseUrl: process.env.VUE_APP_API_BASE_URL,
	token: token,
	user: localStorage.getItem('user') ? JSON.parse(localStorage.getItem('user')) : null,
	colorScheme: localStorage.getItem('colorScheme') || 'default',
	beamsClient: new PusherPushNotifications.Client({
		instanceId: process.env.VUE_APP_PUSHER_BEAMS_INSTANCE_ID,
	}),
	tokenProvider: new PusherPushNotifications.TokenProvider({
		url: process.env.VUE_APP_API_BASE_URL + 'pusher/beams-auth',
		headers: {
			Authorization: 'Bearer ' + token.token
		}
	}),
	userSettings: {
		showTooltips: true
	}
};

const getters = {
	apiBaseUrl: state => state.apiBaseUrl,
	token: state => state.token,
	beamsClient: state => state.beamsClient,
	tokenProvider: state => state.tokenProvider,
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
	setBeamsClient (state, beamsClient) {
		state.beamsClient = beamsClient
	}
}

const actions = {
	logout() {
		localStorage.clear()
		document.location.reload()
	},
	async loadUserSettings ({ commit }) {
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
