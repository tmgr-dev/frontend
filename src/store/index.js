import { createStore } from 'vuex'
import colorSchemes from '../colors/schemes'

const color = (colorKey, colorScheme) => colorSchemes[colorScheme][colorKey]

const state = {
	apiBaseUrl: process.env.VUE_APP_API_BASE_URL,
	token: localStorage.getItem('token') ? JSON.parse(localStorage.getItem('token')) : null,
	user: localStorage.getItem('user') ? JSON.parse(localStorage.getItem('user')) : null,
	colorScheme: localStorage.getItem('colorScheme') || 'default',
};

const getters = {
	apiBaseUrl: state => state.apiBaseUrl,
	token: state => state.token,
	user: state => state.user,
	isLoggedIn: state => state.token !== null,
	colorScheme: state => state.colorScheme
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
	}
}

const actions = {
	logout({ commit }) {
		commit('token', null)
		commit('colorScheme', 'default')
	}
}

export default createStore({
	state,
	getters,
	mutations,
	actions
})
