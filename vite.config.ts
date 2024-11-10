import { defineConfig } from 'vite';
import vue from '@vitejs/plugin-vue';
import { fileURLToPath } from 'node:url';

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
	},
});
