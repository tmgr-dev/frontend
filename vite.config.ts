import { defineConfig } from 'vite';
import vue from '@vitejs/plugin-vue';
import { fileURLToPath } from 'node:url';
import tailwind from 'tailwindcss';
import autoprefixer from 'autoprefixer';

// @todo replace build command in package.json later
// "build": "vue-tsc -b && vite build",

// https://vite.dev/config/
export default defineConfig({
	plugins: [vue()],
	resolve: {
		alias: [
			{
				find: '@',
				replacement: fileURLToPath(new URL('./src', import.meta.url)),
			},
		],
	},
	css: {
		preprocessorOptions: {
			scss: {
				quietDeps: true,
				logger: {
					warn: () => {}, // Отключает все предупреждения
				},
				silenceDeprecations: ['legacy-js-api'],
			},
		},
		postcss: {
			plugins: [tailwind, autoprefixer],
		},
	},
});
