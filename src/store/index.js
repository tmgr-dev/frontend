import Vue from '../bootstrap/index'
import Vuex from 'vuex'
import colorSchemes from '../colors/schemes'
import axios from "axios";
const color = (colorKey, colorScheme) => colorSchemes[colorScheme][colorKey]

const axiosWithState = (state) => {
	axios.defaults.baseURL = state.apiBaseUrl
	axios.defaults.headers = {
		Authorization: `Bearer ${state.token.token}`,
		'X-Requested-With': 'XMLHttpRequest'
	}
	return axios
}
Vue.use(Vuex)

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
    token (state, token) {
        if (token == null) {
            localStorage.removeItem('token')
        } else {
            localStorage.setItem('token', JSON.stringify(token))
        }

        state.token = token
    },
    user (state, user) {
        if (user == null) {
            localStorage.removeItem('user')
        } else {
            localStorage.setItem('user', JSON.stringify(user))
        }

        state.user = user
    },
    colorScheme (state, colorScheme) {
        if (colorScheme == null) {
            localStorage.removeItem('colorScheme')
        } else {
            localStorage.setItem('colorScheme', colorScheme)
        }

				axiosWithState(state).patch('/user/settings', {
					settings: {
						colorScheme: colorScheme
					}
				})

        state.colorScheme = colorScheme
        document.querySelector('body').className = color('bgBody', colorScheme)
    }
}

const actions = {
    logout (context) {
        context.commit('token', null)
    }
}

export default new Vuex.Store({
    state,
    getters,
    mutations,
    actions
})
