/**
 * TODO: Needs refactoring
 */

import { createApp, h } from 'vue'

// Vue Application
import App from './App'

// HTTP client
import axios from 'axios'

// Vue plugins
import router from './routes/index'
import store from './store'

// Color schemes
import colorSchemes from './colors/schemes'

// Directives
import { mask } from 'vue-the-mask'
import Tooltip from '@/directives/tooltip/src';
import Selectable from '@/directives/selectable/src';

// Load components
import components from "@/bootstrap/globalComponents";

// Styles
import './assets/styles/index.scss';

const app = createApp(App)

components.map(component => app.component(component.name, component))

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
app.use(Selectable)

app.mount('#app')

export default app
