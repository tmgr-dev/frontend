import Vue from './bootstrap/index'
import router from './routes/index'
import App from './App'
import './assets/styles/index.scss';
import axios from 'axios'
import store from './store'
import colorSchemes from './colors/schemes'
import VueTheMask from 'vue-the-mask';
import { library } from '@fortawesome/fontawesome-svg-core'
import {faPlusCircle, faPencilAlt, faBars, faEgg, faBackward} from '@fortawesome/free-solid-svg-icons'
import { FontAwesomeIcon } from '@fortawesome/vue-fontawesome'


library.add(faPlusCircle)
library.add(faEgg)
library.add(faPencilAlt)
library.add(faBars)
library.add(faBackward)

Vue.component('font-awesome-icon', FontAwesomeIcon)

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
