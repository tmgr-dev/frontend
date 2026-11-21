import { createHash } from 'node:crypto';
import { readFileSync } from 'node:fs';
import { join } from 'node:path';

const root = join(__dirname, '..', '..');
const read = (path: string) => readFileSync(join(root, path), 'utf8');

const page = read('public/pair.html');
const conf = read('docker/nginx/conf.d/default.conf.example');
const aasa = JSON.parse(read('public/.well-known/apple-app-site-association'));

const inlineScripts = [
	...page.matchAll(/<script(\s[^>]*)?>([\s\S]*?)<\/script>/gi),
];

describe('public/pair.html', () => {
	it('has no external scripts', () => {
		expect(page).not.toMatch(/<script[^>]*\ssrc\s*=/i);
	});

	it.each([
		'googletagmanager',
		'google-analytics',
		'gtag',
		'plausible',
		'yandex',
		'metrika',
		'mc.yandex',
	])('has no %s marker', (marker) => {
		expect(page.toLowerCase()).not.toContain(marker);
	});

	it('never mentions the tmgr:// scheme', () => {
		expect(page).not.toContain('tmgr://');
	});

	it('has no external stylesheets, fonts or images', () => {
		expect(page).not.toMatch(/<link[^>]+rel=["']?stylesheet/i);
		expect(page).not.toMatch(/(src|srcset)=["']?(https?:)?\/\//i);
	});

	it('carries the privacy meta tags', () => {
		expect(page).toContain('<meta name="referrer" content="no-referrer">');
		expect(page).toContain('<meta name="robots" content="noindex">');
	});

	it('has exactly one inline script that clears the fragment', () => {
		expect(inlineScripts).toHaveLength(1);
		expect(inlineScripts[0][2]).toBe("history.replaceState(null,'','/pair')");
	});

	it('has its script hash in the nginx CSP', () => {
		const hash = createHash('sha256')
			.update(inlineScripts[0][2])
			.digest('base64');
		expect(conf).toContain(`script-src 'sha256-${hash}'`);
	});
});

describe('apple-app-site-association', () => {
	it('allows only the /pair path for the iOS app', () => {
		expect(aasa.applinks.details).toHaveLength(1);
		const [detail] = aasa.applinks.details;
		expect(detail.appIDs).toEqual(['N4HM7NF9ZZ.dev.tmgr']);
		expect(detail.components).toEqual([{ '/': '/pair', comment: 'pairing' }]);
	});
});

describe('nginx pairing rules', () => {
	const block = (head: string) => {
		const start = conf.indexOf(head);
		expect(start).toBeGreaterThan(-1);
		return conf.slice(start, conf.indexOf('}', start) + 1);
	};

	it('serves the AASA file as json without an index.html fallback', () => {
		const rule = block('location = /.well-known/apple-app-site-association');
		expect(rule).toContain('default_type application/json');
		expect(rule).toContain('try_files $uri =404');
		expect(rule).not.toContain('index.html');
	});

	it('serves /pair statically with no-store, no-referrer and a CSP', () => {
		const rule = block('location ^~ /pair');
		expect(rule).toContain('try_files /pair.html =404');
		expect(rule).toContain('Cache-Control "no-store" always');
		expect(rule).toContain('Referrer-Policy "no-referrer" always');
		expect(rule).toContain("default-src 'none'");
		expect(rule).toContain("frame-ancestors 'none'");
		expect(rule).not.toContain('index.html');
	});

	it('hides pair.html itself and orders /pair before the SPA fallback', () => {
		expect(conf).toContain('location = /pair.html { return 404; }');
		expect(conf.indexOf('location ^~ /pair')).toBeLessThan(
			conf.indexOf('location / {'),
		);
	});
});
