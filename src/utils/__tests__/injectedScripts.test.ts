import { findInjectedScripts } from '../injectedScripts';

const PAGE = 'https://tmgr.dev/tmgrdev/tasks/1';

describe('findInjectedScripts', () => {
	it('returns [] for empty input', () => {
		expect(findInjectedScripts([], PAGE)).toEqual([]);
	});

	it('ignores our own scripts and inline scripts', () => {
		expect(
			findInjectedScripts(
				[
					'',
					'https://tmgr.dev/assets/js/main-abc123.js',
					'https://tmgr.dev/assets/js/chunks/Board-d4e5.js',
					'https://tmgr.dev/registerSW.js',
					'https://tmgr.dev/sw.js',
					'https://tmgr.dev/workbox-9f8e7d.js',
				],
				PAGE,
			),
		).toEqual([]);
	});

	it('ignores cross-origin scripts', () => {
		expect(
			findInjectedScripts(
				[
					'https://analytics.dzik.email/js/script.js',
					'https://telegram.org/js/telegram-widget.js?22',
					'https://www.googletagmanager.com/gtag/js?id=G-XXXX',
				],
				PAGE,
			),
		).toEqual([]);
	});

	it('flags a same-origin script at an unknown path', () => {
		expect(
			findInjectedScripts(
				['https://tmgr.dev/assets/js/main-abc.js', 'https://tmgr.dev/-F3-7My'],
				PAGE,
			),
		).toEqual(['/-F3-7My']);
	});

	it('strips query and hash', () => {
		expect(
			findInjectedScripts(['https://tmgr.dev/inject.js?v=1#x'], PAGE),
		).toEqual(['/inject.js']);
	});

	it('truncates paths longer than 40 chars', () => {
		const exact = `/${'a'.repeat(39)}`;
		const long = `/${'b'.repeat(40)}`;
		expect(
			findInjectedScripts(
				[`https://tmgr.dev${exact}`, `https://tmgr.dev${long}`],
				PAGE,
			),
		).toEqual([exact, `/${'b'.repeat(39)}…`]);
	});

	it('ignores malformed, blob: and data: srcs without throwing', () => {
		expect(
			findInjectedScripts(
				[
					'http://',
					'not a url',
					'blob:https://tmgr.dev/0b1c-uuid',
					'data:text/javascript,alert(1)',
				],
				PAGE,
			),
		).toEqual([]);
		expect(findInjectedScripts(['https://tmgr.dev/x.js'], '::bad')).toEqual([]);
	});

	it('ignores desktop pages and their assets', () => {
		expect(
			findInjectedScripts(
				['tauri://localhost/assets/js/main-abc.js', 'tauri://localhost/x.js'],
				'tauri://localhost/',
			),
		).toEqual([]);
		expect(
			findInjectedScripts(
				['http://tauri.localhost/assets/js/main-abc.js'],
				'http://tauri.localhost/',
			),
		).toEqual([]);
	});

	it('ignores the same host on a different protocol', () => {
		expect(findInjectedScripts(['http://tmgr.dev/inject.js'], PAGE)).toEqual(
			[],
		);
	});

	it('compares host with port on http pages', () => {
		const page = 'http://localhost:4173/';
		expect(
			findInjectedScripts(
				[
					'http://localhost:4173/inject.js',
					'http://localhost:5173/other-port.js',
					'http://localhost:4173/assets/js/main-abc.js',
				],
				page,
			),
		).toEqual(['/inject.js']);
	});

	it('flags lookalike paths outside the allowlist', () => {
		expect(
			findInjectedScripts(
				[
					'https://tmgr.dev/assetsX/a.js',
					'https://tmgr.dev/x/registerSW.js',
					'https://tmgr.dev/x/workbox-1.js',
					'https://tmgr.dev/sw.js.map',
				],
				PAGE,
			),
		).toEqual([
			'/assetsX/a.js',
			'/x/registerSW.js',
			'/x/workbox-1.js',
			'/sw.js.map',
		]);
	});

	it('ignores javascript: and file: srcs', () => {
		expect(
			findInjectedScripts(['javascript:void 0', 'file:///x.js'], PAGE),
		).toEqual([]);
	});
});
