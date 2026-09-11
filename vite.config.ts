import { defineConfig } from 'vite';
import vue from '@vitejs/plugin-vue';
import { VitePWA } from 'vite-plugin-pwa';
import { fileURLToPath } from 'node:url';
import tailwind from 'tailwindcss';
import autoprefixer from 'autoprefixer';

const escapeRegExp = (value: string) => value.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');

/** One codeSplitting group: every module under node_modules/<pkg>/ for the listed packages. */
const vendorGroup = (name: string, packages: string[], priority: number) => ({
	name,
	priority,
	test: new RegExp(`[\\\\/]node_modules[\\\\/](${packages.map(escapeRegExp).join('|')})[\\\\/]`),
});

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
				globIgnores: ['**/assets/**/{inter,jetbrains-mono,quicksand,instrument-serif}-*.woff2'],
				importScripts: ['https://js.pusher.com/beams/service-worker.js'],
				runtimeCaching: [
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
		rolldownOptions: {
			output: {
				// Vite 8 builds with rolldown: the object form of manualChunks is gone and the
				// function form is deprecated, so the same package -> chunk map is expressed as
				// codeSplitting groups. Higher priority wins when a module matches several.
				codeSplitting: {
					groups: [
						vendorGroup('vendor-vue', ['vue', 'vue-router', 'vuex'], 60),
						vendorGroup(
							'vendor-editor',
							[
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
							50,
						),
						vendorGroup(
							'vendor-ui',
							[
								'@headlessui/vue',
								'radix-vue',
								'@vueuse/core',
								'lucide-vue-next',
								'@radial-color-picker/vue-color-picker',
							],
							40,
						),
						vendorGroup(
							'vendor-utils',
							['axios', 'date-fns', 'canvas-confetti', 'vuedraggable'],
							30,
						),
						vendorGroup('vendor-markdown', ['md-editor-v3'], 20),
						vendorGroup(
							'vendor-pusher',
							['@pusher/push-notifications-web', 'pusher-js', 'laravel-echo'],
							10,
						),
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
