import { defineConfig } from 'vite';
import vue from '@vitejs/plugin-vue';
import { VitePWA } from 'vite-plugin-pwa';
import { fileURLToPath } from 'node:url';
import tailwind from 'tailwindcss';
import autoprefixer from 'autoprefixer';

export default defineConfig({
	plugins: [
		vue(),
		VitePWA({
			registerType: 'autoUpdate',
			devOptions: {
				enabled: true,
			},
			includeAssets: ['favicon.ico', 'apple-touch-icon.png', 'safari-pinned-tab.svg'],
			manifest: {
				name: 'TMGR - Task Manager',
				short_name: 'TMGR',
				description: 'Task management system with time tracking',
				theme_color: '#555555',
				background_color: '#1a1a1a',
				display: 'standalone',
				icons: [
					{
						src: 'favicon-32x32.png',
						sizes: '32x32',
						type: 'image/png',
					},
					{
						src: 'apple-touch-icon.png',
						sizes: '180x180',
						type: 'image/png',
					},
					{
						src: 'android-chrome-192x192.png',
						sizes: '192x192',
						type: 'image/png',
					},
					{
						src: 'android-chrome-512x512.png',
						sizes: '512x512',
						type: 'image/png',
						purpose: 'any maskable',
					},
				],
			},
			workbox: {
				globPatterns: ['**/*.{js,css,html,ico,png,svg,woff2}'],
				importScripts: ['https://js.pusher.com/beams/service-worker.js'],
				runtimeCaching: [
					{
						urlPattern: /^https:\/\/fonts\.googleapis\.com\/.*/i,
						handler: 'CacheFirst',
						options: {
							cacheName: 'google-fonts-stylesheets',
							expiration: {
								maxEntries: 10,
								maxAgeSeconds: 60 * 60 * 24 * 365,
							},
						},
					},
					{
						urlPattern: /^https:\/\/fonts\.gstatic\.com\/.*/i,
						handler: 'CacheFirst',
						options: {
							cacheName: 'google-fonts-webfonts',
							expiration: {
								maxEntries: 30,
								maxAgeSeconds: 60 * 60 * 24 * 365,
							},
						},
					},
					{
						urlPattern: /\/api\/.*/i,
						handler: 'NetworkFirst',
						options: {
							cacheName: 'api-cache',
							expiration: {
								maxEntries: 100,
								maxAgeSeconds: 60 * 5,
							},
							networkTimeoutSeconds: 10,
						},
					},
				],
			},
		}),
	],
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
