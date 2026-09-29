import { readFileSync } from 'node:fs';
import { join } from 'node:path';

type Entry = { url: string; revision: string | null; size: number };

export const entryGraphTransform =
	(readIndexHtml: () => string) => async (entries: Entry[]) => {
		const html = readIndexHtml();
		const manifest = entries.filter(
			(entry) =>
				!entry.url.startsWith('assets/') || html.includes(`/${entry.url}`),
		);
		return { manifest, warnings: [] as string[] };
	};

export const pwaWorkbox = {
	globPatterns: ['**/*.{js,css,html,ico,png,svg,woff2,webmanifest}'],
	manifestTransforms: [
		entryGraphTransform(() =>
			readFileSync(join(process.cwd(), 'dist', 'index.html'), 'utf8'),
		),
	],
	globIgnores: [
		'**/assets/**/{exo-2,jetbrains-mono,quicksand,instrument-serif}-*.woff2',
	],
	navigateFallbackDenylist: [
		/^\/(login|register)(\/|\?|$)/i,
		/^\/desktop-auth(\/|\?|$)/i,
	],
	importScripts: [
		'clear-private-api-cache.js',
		'https://js.pusher.com/beams/service-worker.js',
	],
	runtimeCaching: [
		{ urlPattern: /\/api\/.*/i, handler: 'NetworkOnly' as const },
		{
			urlPattern: ({ url, sameOrigin }: { url: URL; sameOrigin: boolean }) =>
				sameOrigin && url.pathname.startsWith('/assets/'),
			handler: 'CacheFirst' as const,
			options: {
				cacheName: 'build-assets',
				expiration: {
					maxEntries: 400,
					maxAgeSeconds: 30 * 24 * 60 * 60,
				},
			},
		},
	],
};
