import '@/assets/styles/_design-tokens.scss';
import '@fontsource/exo-2/400.css';
import '@fontsource/exo-2/600.css';
import { createApp } from 'vue';
import App from './App.vue';

const scheme = window.matchMedia('(prefers-color-scheme: light)');
const applyScheme = () =>
	document.documentElement.classList.toggle('dark', !scheme.matches);
applyScheme();
scheme.addEventListener('change', applyScheme);

createApp(App).mount('#root');
