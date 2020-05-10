import Vue from './bootstrap/index'
import router from './routes/index'
import App from './App'
import './assets/styles/index.scss';
import axios from 'axios'
import store from './store'

Vue.config.productionTip = false

axios.defaults.baseURL = store.getters.apiBaseUrl
if (store.getters.token) {
    axios.defaults.headers = {
        Authorization: `Bearer ${store.getters.token.token}`,
        'X-Requested-With': 'XMLHttpRequest'
    }
    axios.get('user').then(({ data }) => {
        store.commit('user', data)
    })
}

Vue.prototype.$axios = axios

new Vue({
    router,
    store,
    render: h => h(App),
    components: {

    },
    data () {
        return {
            key: null
        }
    }
}).$mount('#app')
