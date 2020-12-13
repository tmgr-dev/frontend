import { createApp, h } from 'vue'
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
import VueSelect from 'vue-select'
import 'vue-select/dist/vue-select.css';
import VueDraggableResizable from 'vue-draggable-resizable'

// optionally import default styles
import 'vue-draggable-resizable/dist/VueDraggableResizable.css'

const app = createApp(App)

components.map(component => app.component(component.name, component))
app.component('vselect', VueSelect)
app.component('vue-draggable-resizable', VueDraggableResizable)
app.use(router)
app.use(store)

axios.defaults.baseURL = store.getters.apiBaseUrl
if (store.getters.token) {
	axios.defaults.headers = {
		Authorization: `Bearer ${store.getters.token.token}`,
		'X-Requested-With': 'XMLHttpRequest',
		'Cache-Control': 'no-cache',
		'Pragma': 'no-cache',
		'Expires': '0'
	}
	axios.get('user').then(({ data }) => {
		store.commit('user', data)
	})
}
app.config.globalProperties.$axios = axios

const color = colorKey => colorSchemes[store.getters.colorScheme][colorKey]
app.config.globalProperties.$color = color
document.querySelector('body').className = color('bgBody')

app.directive('mask', mask)
app.use(Tooltip, {
	delay: 50,
	placement: 'top',
	class: 'custom-tooltip',
	triggers: ['hover'],
	offset: 5
})
app.use(VueTheMask)
app.mount('#app')

export default app
