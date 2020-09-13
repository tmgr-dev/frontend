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
/*Vue.directive('tooltip', {
  bind (el, { value }) {
    el.classList.add('relative')
    if (!el.querySelector('.tooltip')) {
      el.insertAdjacentHTML("beforeend", `
        <div class="tooltip">
          <span class="triangle"></span>
          ${value}
        </div>`)

      const tooltip = el.querySelector('.tooltip')
      if (tooltip) {
        el.addEventListener('mouseover', () => {
          tooltip.classList.add('active')
        })

        el.addEventListener('mouseout', () => {
          tooltip.classList.remove('active')
        })
      }
    }
  }
})*/

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
