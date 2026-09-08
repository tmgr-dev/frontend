import alertPlugin from '@/alert';
import components from '@/globalComponents.ts';
import { mask, VueTheMask } from '@/plugins/VueTheMask';
import Selectable from '@/plugins/directives/selectable';
import router from '@/router';
import store from '@/store';
import { createApp } from 'vue';
import App from './App.vue';

store.commit('setColorScheme', localStorage.getItem('colorScheme'));
store.commit('setTheme', localStorage.getItem('theme') || 'default');

const app = createApp(App);

components.map((component) => app.component(component.name || '', component));
app.component('VueTheMask', VueTheMask);
app.directive('mask', mask);
app.use(Selectable);
app.use(router);
app.use(store);

alertPlugin({ app });

app.mount('#app');
