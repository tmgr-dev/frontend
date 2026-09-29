import { entryGraphTransform, pwaWorkbox } from '../pwaWorkbox';

const denylist: RegExp[] = (pwaWorkbox as any).navigateFallbackDenylist ?? [];
const denied = (path: string) => denylist.some((re) => re.test(path));
const AAA = 'A'.repeat(43);

describe('pwaWorkbox navigateFallbackDenylist', () => {
	it.each([
		`/login/github?code=x&state=desktop.${AAA}`,
		`/login/google?code=x&state=desktop.${AAA}`,
		'/login',
		'/login?x=1',
		'/register',
		'/register/anything',
		'/LOGIN/github?code=x',
		'/Desktop-Auth/return',
		'/desktop-auth/return',
		'/desktop-auth/telegram?tx=1',
	])('denies %s', (path) => {
		expect(denied(path)).toBe(true);
	});

	it.each([
		'/',
		'/tmgrdev/list',
		'/tmgrdev/tasks/1',
		'/loginx',
		'/registered',
		'/tmgrdev/login',
	])('allows %s', (path) => {
		expect(denied(path)).toBe(false);
	});
});

describe('pwaWorkbox precache', () => {
	const entry = (url: string) => ({ url, revision: null, size: 1 });
	const html =
		'<script src="/assets/js/index-A.js"></script>' +
		'<link rel="modulepreload" href="/assets/js/vendor-B.js">' +
		'<link rel="stylesheet" href="/assets/css/index-C.css">' +
		'<link rel="preload" href="/assets/woff2/material-icons-D.woff2">';

	it('keeps root files and only the assets index.html references', async () => {
		const { manifest } = await entryGraphTransform(() => html)(
			[
				'index.html',
				'favicon.ico',
				'manifest.webmanifest',
				'assets/js/index-A.js',
				'assets/js/vendor-B.js',
				'assets/css/index-C.css',
				'assets/woff2/material-icons-D.woff2',
				'assets/js/lazy-E.js',
				'assets/css/lazy-F.css',
			].map(entry),
		);
		expect(manifest.map((e) => e.url)).toEqual([
			'index.html',
			'favicon.ico',
			'manifest.webmanifest',
			'assets/js/index-A.js',
			'assets/js/vendor-B.js',
			'assets/css/index-C.css',
			'assets/woff2/material-icons-D.woff2',
		]);
	});

	it('is wired into manifestTransforms and globs cover assets', () => {
		expect(pwaWorkbox.manifestTransforms).toHaveLength(1);
		expect(pwaWorkbox.globPatterns.join(' ')).toMatch(/js.*css.*html/);
	});
});

describe('pwaWorkbox runtime caching', () => {
	const routes = pwaWorkbox.runtimeCaching as any[];
	const match = (path: string) =>
		routes.find((r) => {
			const url = new URL(path, 'https://tmgr.dev');
			return typeof r.urlPattern === 'function'
				? r.urlPattern({ url, sameOrigin: true })
				: (r.urlPattern as RegExp).test(url.href);
		});

	it('serves hashed build assets cache-first with expiration', () => {
		const route = match('/assets/js/index-abc123.js');
		expect(route.handler).toBe('CacheFirst');
		expect(match('/assets/css/x-1.css').handler).toBe('CacheFirst');
		expect(match('/assets/anything').handler).toBe('CacheFirst');
		expect(match('/tmgrdev/list')).toBeUndefined();
		expect(route.options.cacheName).toEqual(expect.any(String));
		expect(route.options.expiration.maxEntries).toBeGreaterThan(0);
		expect(route.options.expiration.maxAgeSeconds).toBeGreaterThan(0);
	});

	it('keeps api network-only', () => {
		expect(match('/api/tasks').handler).toBe('NetworkOnly');
	});
});
