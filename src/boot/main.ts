import axios from 'axios';
import { Component } from '@vue/runtime-core';

// Directives
import { mask, VueTheMask } from 'src/plugins/VueTheMask';
import { QuillEditor } from '@vueup/vue-quill';
import '@vueup/vue-quill/dist/vue-quill.bubble.css';
import Tooltip from 'src/plugins/directives/tooltip';
import Selectable from 'src/plugins/directives/selectable';

import components from 'src/boot/globalComponents';

export default (obj: any) => {
	const { app, router, store } = obj;

	components.map((component: Component) =>
		app.component(component.name || '', component),
	);
	app.component('VueTheMask', VueTheMask);
	app.component('QuillEditor', QuillEditor);

	store.commit('setColorScheme', localStorage.getItem('colorScheme'));

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
