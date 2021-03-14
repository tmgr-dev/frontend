// HTTP client
import axios from 'axios';

import { Component } from "@vue/runtime-core";

// Dark & Day modes
import colorSchemes from "src/colors/schemes";

// Directives
import { VueTheMask, mask } from "src/components/UIElements/VueTheMask";
import Tooltip from "src/directives/tooltip";
import Selectable from "src/directives/selectable";

import components from "src/bootstrap/globalComponents";


export default (obj: any) => {
	const { app, router, store } = obj;

	components.map((component: Component) => app.component(component.name || '', component));
	app.component('VueTheMask', VueTheMask);

	axios.defaults.baseURL = store.getters.apiBaseUrl;
	if (store.getters.token) {
		axios.defaults.headers = {
			Authorization: `Bearer ${store.getters.token.token}`,
			'X-Requested-With': 'XMLHttpRequest',
			'Cache-Control': 'no-cache',
			'Pragma': 'no-cache',
			'Expires': '0'
		}
	}
	app.config.globalProperties.$axios = axios;

	const color = (colorKey: string) => colorSchemes[store.getters.colorScheme][colorKey];
	app.config.globalProperties.$color = color;

	const body = document.querySelector('body');
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
}
