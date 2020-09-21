import Vue from './bootstrap/index'
import router from './routes/index'
import App from './App'
import './assets/styles/index.scss';
import axios from 'axios'
import store from './store'
import colorSchemes from './colors/schemes'
import VueTheMask from 'vue-the-mask';
import { mask } from 'vue-the-mask'
import Tooltip from 'vue-directive-tooltip';

Vue.config.productionTip = false

axios.defaults.baseURL = store.getters.apiBaseUrl
if (store.getters.token) {
    axios.defaults.headers = {
        Authorization: `Bearer ${store.getters.token.token}`,
        'X-Requested-With': 'XMLHttpRequest'
    }
    axios.get('user').then(({ data }) => {
        store.commit('user', data)
    }).finally(() => {
			axios.get('user/settings').then(({ data: {data} }) => {
				if (data && data.settings && data.settings.colorScheme) {
					store.commit('colorScheme', data.settings.colorScheme)
				}
			})
		})
}
Vue.directive(mask)
Vue.use(Tooltip, {
  delay: 50,
  placement: 'top',
  class: 'custom-tooltip',
  triggers: ['hover'],
  offset: 5
})
Vue.use(VueTheMask)

Vue.prototype.$axios = axios
const color = colorKey => colorSchemes[store.getters.colorScheme][colorKey]
Vue.prototype.$color = color

document.querySelector('body').className = color('bgBody')

new Vue({
  router,
  store,
  render: h => h(App),
  components: {

  },
  data: () => ({
    key: null
  })
}).$mount('#app')
