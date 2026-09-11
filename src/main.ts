import '@fontsource/inter/400.css';
import '@fontsource/inter/500.css';
import '@fontsource/inter/600.css';
import '@fontsource/inter/700.css';
import '@fontsource/jetbrains-mono/400.css';
import '@fontsource/jetbrains-mono/500.css';
import '@fontsource/quicksand/400.css';
import '@fontsource/quicksand/500.css';
import '@fontsource/quicksand/600.css';
import '@fontsource/quicksand/700.css';
import '@fontsource/instrument-serif/400.css';
import '@fontsource/instrument-serif/400-italic.css';
import alertPlugin from '@/alert';
import components from '@/globalComponents.ts';
import { mask, VueTheMask } from '@/plugins/VueTheMask';
import Selectable from '@/plugins/directives/selectable';
import router from '@/router';
import store from '@/store';
import { tokenFromStorageEvent } from '@/utils/tokenSync';
import { createApp } from 'vue';
import App from './App.vue';

store.commit('setColorScheme', localStorage.getItem('colorScheme'));
store.commit('setTheme', localStorage.getItem('theme') || 'default');

// Another tab rotated the token (or logged out): adopt it here so this tab
// never replays a retired refresh token. setToken re-persists the same value,
// which fires no storage event in this tab and a no-op one in the others.
window.addEventListener('storage', (event) => {
	const result = tokenFromStorageEvent(event, store.state.token);
	if (result.changed) {
		store.commit('setToken', result.token);
	}
});

const app = createApp(App);

components.map((component) => app.component(component.name || '', component));
app.component('VueTheMask', VueTheMask);
app.directive('mask', mask);
app.use(Selectable);
app.use(router);
app.use(store);

alertPlugin({ app });

app.mount('#app');
