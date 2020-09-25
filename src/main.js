import { createApp } from "vue";
import router from "./routes/index";
import App from "./App";
import axios from "axios";
import store from "./store";
import VueTheMask from "vue-the-mask";
import { mask } from "vue-the-mask";
import Tooltip from "vue-directive-tooltip";
import components from "@/bootstrap/loadComponents";
import colorSchemes from "@/colors/schemes";

const app = createApp(App, {
	data: () => ({
		key: null
	})
})
app.use(store)
app.use(router)

components.map(c => app.component(c.name, c))

axios.defaults.baseURL = store.getters.apiBaseUrl
console.log(axios.defaults.baseURL)
if (store.getters.token) {
	axios.defaults.headers = {
		Authorization: `Bearer ${store.getters.token.token}`,
		'X-Requested-With': 'XMLHttpRequest'
	}
	axios.get('user').then(({ data }) => {
		store.commit('user', data)
	})
}
app.config.globalProperties.$axios = axios

const color = colorKey => colorSchemes[store.getters.colorScheme][colorKey]
app.config.globalProperties.$color = color
document.querySelector('body').className = color('bgBody')

app.directive(mask)
app.use(Tooltip, {
  delay: 50,
  placement: 'top',
  class: 'custom-tooltip',
  triggers: ['hover'],
  offset: 5
})
app.use(VueTheMask)

router.isReady().then(() => {
	app.mount('#app')
})
