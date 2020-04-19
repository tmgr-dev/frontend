import Vue from '../bootstrap/index'
import Vuex from 'vuex'

Vue.use(Vuex)

const state = {
    apiBaseUrl: process.env.VUE_APP_API_BASE_URL,
    token: localStorage.getItem('token') ? JSON.parse(localStorage.getItem('token')) : null,
    user: localStorage.getItem('user') ? JSON.parse(localStorage.getItem('user')) : null
};

const getters = {
    apiBaseUrl: state => state.apiBaseUrl,
    token: state => state.token,
    user: state => state.user,
    isLoggedIn: state => state.token !== null
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
