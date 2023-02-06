import axios from 'axios';
import { Component } from '@vue/runtime-core';

// Directives
import { mask, VueTheMask } from 'src/plugins/VueTheMask';
import { QuillEditor } from '@vueup/vue-quill';
import '@vueup/vue-quill/dist/vue-quill.bubble.css';
import Tooltip from 'src/directives/tooltip';
import Selectable from 'src/directives/selectable';

import components from 'src/bootstrap/globalComponents';

export default (obj: any) => {
	const { app, router, store } = obj;

	components.map((component: Component) =>
		app.component(component.name || '', component),
	);
	app.component('VueTheMask', VueTheMask);
	app.component('QuillEditor', QuillEditor);

	store.commit('colorScheme', localStorage.getItem('colorScheme'));

	app.directive('mask', mask);
	app.use(Tooltip, {
		delay: 50,
		placement: 'top',
		class: 'custom-tooltip',
		triggers: ['hover'],
		offset: 5,
	});
	app.use(Selectable);
	app.use(router);
	app.use(store);
};
