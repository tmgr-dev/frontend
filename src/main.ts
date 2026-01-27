import { createApp } from 'vue';
import { mask, VueTheMask } from '@/plugins/VueTheMask';
import Tooltip from '@/plugins/directives/tooltip';
import Selectable from '@/plugins/directives/selectable';
import App from './App.vue';
import components from '@/globalComponents.ts';
import store from '@/store';
import router from '@/router';
import alertPlugin from '@/alert';

store.commit('setColorScheme', localStorage.getItem('colorScheme'));

const app = createApp(App);

components.map((component) => app.component(component.name || '', component));
app.component('VueTheMask', VueTheMask);
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

alertPlugin({ app });

app.mount('#app');
