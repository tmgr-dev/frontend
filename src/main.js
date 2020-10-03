import Vue from 'vue'
import router from './routes/index'
import App from './App'
import './assets/styles/index.scss';
import axios from 'axios'
import store from './store'
import colorSchemes from './colors/schemes'
import VueTheMask from 'vue-the-mask';
import { mask } from 'vue-the-mask'
import Tooltip from 'vue-directive-tooltip';
import components from "@/bootstrap/globalComponents";
import VueMeta from 'vue-meta';
import VueSelect from 'vue-select'
import 'vue-select/dist/vue-select.css';

Vue.config.productionTip = false

components.map(component => Vue.component(component.name, component))
Vue.component('vselect', VueSelect)
Vue.use(VueMeta)

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

const color = colorKey => colorSchemes[store.getters.colorScheme][colorKey]
Vue.prototype.$color = color
document.querySelector('body').className = color('bgBody')

Vue.directive(mask)
Vue.use(Tooltip, {
  delay: 50,
  placement: 'top',
  class: 'custom-tooltip',
  triggers: ['hover'],
  offset: 5
})
Vue.use(VueTheMask)

new Vue({
  router,
  store,
  render: h => h(App),
  data: () => ({
    key: null
  })
}).$mount('#app')
