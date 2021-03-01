
/**
 * TODO: Needs refactoring
 */

import { createApp } from "vue";
import { Component } from "@vue/runtime-core";
// Vue Application
import App from "./App.vue";

// HTTP client
import axios from "axios";
import colorSchemes from "./colors/schemes";

// Vue plugins
const {default: router} = require('./router');
const {default: store} = require('./store');

// Color schemes


// Directives
const { VueTheMask, mask } = require('./components/UIElements/VueTheMask/index');
const {default: Tooltip} = require('src/directives/tooltip/index');
const {default: Selectable} = require('src/directives/selectable/index');

// Load components
const {default: components} = require("src/bootstrap/globalComponents");

const app = createApp(App);

components.map((component: Component) => app.component(component.name || '', component));
app.component(VueTheMask.name, VueTheMask);
app.use(router);
app.use(store);

axios.defaults.baseURL = store.getters.apiBaseUrl;
if (store.getters.token) {
	axios.defaults.headers = {
		Authorization: `Bearer ${store.getters.token.token}`,
		'X-Requested-With': 'XMLHttpRequest',
		'Cache-Control': 'no-cache',
		'Pragma': 'no-cache',
		'Expires': '0'
	};
	axios.get('user').then(({ data }: any) => {
		//store.commit('user', data)
	});
}
app.config.globalProperties.$axios = axios;

const color = (colorKey: string) => colorSchemes[store.getters.colorScheme][colorKey];
app.config.globalProperties.$color = color;
const body: any = document.querySelector('body');
if (body) {
	body.className = color('bgBody');
}

app.directive('mask', mask);

app.use(Tooltip, {
	delay: 50,
	placement: 'top',
	class: 'custom-tooltip',
	triggers: ['hover'],
	offset: 5
})
app.use(Selectable)

app.mount('#q-app');
console.log('test')

export default app;
