import { defineConfig } from 'vite';
import vue from '@vitejs/plugin-vue';
import { fileURLToPath } from 'node:url';
import tailwind from 'tailwindcss';
import autoprefixer from 'autoprefixer';

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
					warn: () => {},
				},
				silenceDeprecations: ['legacy-js-api'],
			},
		},
		postcss: {
			plugins: [tailwind, autoprefixer],
		},
	},
	build: {
		target: 'es2020',
		minify: 'terser',
		terserOptions: {
			compress: {
				drop_console: true,
				drop_debugger: true,
				pure_funcs: ['console.log', 'console.info', 'console.debug'],
			},
		},
		rollupOptions: {
			output: {
				manualChunks: {
					'vendor-vue': ['vue', 'vue-router', 'vuex'],
					'vendor-editor': [
						'@editorjs/editorjs',
						'@editorjs/header',
						'@editorjs/list',
						'@editorjs/checklist',
						'@editorjs/delimiter',
						'@editorjs/embed',
						'@editorjs/inline-code',
						'@editorjs/link',
						'@editorjs/marker',
						'@editorjs/quote',
						'@editorjs/raw',
						'@editorjs/table',
						'@editorjs/warning',
						'@bomdi/codebox',
						'editorjs-drag-drop',
					],
					'vendor-ui': [
						'@headlessui/vue',
						'radix-vue',
						'@vueuse/core',
						'lucide-vue-next',
						'@radial-color-picker/vue-color-picker',
					],
					'vendor-utils': [
						'axios',
						'date-fns',
						'canvas-confetti',
						'vuedraggable',
					],
					'vendor-markdown': ['md-editor-v3'],
					'vendor-pusher': [
						'@pusher/push-notifications-web',
						'pusher-js',
						'laravel-echo',
					],
				},
				chunkFileNames: 'assets/js/[name]-[hash].js',
				entryFileNames: 'assets/js/[name]-[hash].js',
				assetFileNames: 'assets/[ext]/[name]-[hash].[ext]',
			},
		},
		chunkSizeWarningLimit: 500,
		cssCodeSplit: true,
		sourcemap: false,
		assetsInlineLimit: 4096,
	},
	optimizeDeps: {
		include: [
			'vue',
			'vue-router',
			'vuex',
			'axios',
			'@vueuse/core',
		],
	},
});
