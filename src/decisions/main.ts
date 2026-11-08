import '@/assets/styles/_design-tokens.scss';
import { detectLocale, i18n, setLocale, storedLocale } from '@/i18n';
import { followStoredLocale } from '@/i18n/followStoredLocale';
import '@fontsource/exo-2/400.css';
import '@fontsource/exo-2/500.css';
import '@fontsource/exo-2/600.css';
import '@fontsource/exo-2/700.css';
import '@fontsource/jetbrains-mono/400.css';
import '@fontsource/jetbrains-mono/500.css';
import { createApp } from 'vue';
import App from './components/App.vue';
import './decisions.css';

const scheme = window.matchMedia('(prefers-color-scheme: light)');
const applyScheme = () =>
	document.documentElement.classList.toggle('dark', !scheme.matches);
applyScheme();
scheme.addEventListener('change', applyScheme);

followStoredLocale();

void setLocale(
	detectLocale({ stored: storedLocale(), browser: navigator.languages ?? [] }),
)
	.catch(() => undefined)
	.then(() => createApp(App).use(i18n).mount('#root'));
