// HTTP client
import axios from 'axios';

import {Component} from "@vue/runtime-core";

export default (obj: any) => {
	const {app, router, store} = obj;
// Color schemes
	const {default: colorSchemes} = require('src/colors/schemes');

// Directives
	const { VueTheMask, mask } = require('src/components/UIElements/VueTheMask/index');
	const {default: Tooltip} = require('src/directives/tooltip/index');
	const {default: Selectable} = require('src/directives/selectable/index');

// Load components
	const {default: components} = require("src/bootstrap/globalComponents");


	components.map((component: Component) => app.component(component.name || '', component));
	app.component(VueTheMask.name, VueTheMask);

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
	app.use(router)
	app.use(store)

	// app.mount('#q-app');
	console.log('Mounted')
}
