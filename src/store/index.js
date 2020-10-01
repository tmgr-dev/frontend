import Vue from 'vue'
import Vuex from 'vuex'
import colorSchemes from '../colors/schemes'
import axios from 'axios'

const color = (colorKey, colorScheme) => colorSchemes[colorScheme][colorKey]

Vue.use(Vuex)

const state = {
	apiBaseUrl: process.env.VUE_APP_API_BASE_URL,
	token: localStorage.getItem('token') ? JSON.parse(localStorage.getItem('token')) : null,
	user: localStorage.getItem('user') ? JSON.parse(localStorage.getItem('user')) : null,
	colorScheme: localStorage.getItem('colorScheme') || 'default',
	userSettings: {
		showTooltips: true
	}
};

const getters = {
	apiBaseUrl: state => state.apiBaseUrl,
	token: state => state.token,
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
		console.log(colorScheme)
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
	}
}

const actions = {
	logout({ commit }) {
		commit('token', null)
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

export default new Vuex.Store({
	state,
	getters,
	mutations,
	actions
})
