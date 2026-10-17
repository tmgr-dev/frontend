import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import { runInNewContext } from 'node:vm';
import { AUTH_PATH, carriesSecret, stripAnalytics } from '../stripAnalytics';

const html = readFileSync(join(process.cwd(), 'index.html'), 'utf8');

describe('index.html analytics', () => {
	it('web build loads neither Yandex nor Google tags statically', () => {
		expect(html).not.toMatch(
			/mc\.yandex|metrika|ym\(|googletagmanager|gtag\(/i,
		);
	});

	it('web build keeps the self-hosted analytics inside the strip markers', () => {
		expect(html).toContain('analytics.dzik');
		expect(stripAnalytics(html)).not.toContain('analytics.dzik');
	});

	describe('Plausible loader', () => {
		const loader = html.match(
			/<!-- analytics:start -->[\s\S]*?<script>([\s\S]*?)<\/script>[\s\S]*?<!-- analytics:end -->/,
		)![1];

		const injectedOn = (pathname: string) => {
			const appended: Record<string, any>[] = [];
			const document = {
				createElement: () => {
					const el: Record<string, any> = { attrs: {} };
					el.setAttribute = (k: string, v: string) => (el.attrs[k] = v);
					return el;
				},
				head: { appendChild: (el: Record<string, any>) => appended.push(el) },
			};
			runInNewContext(loader, { document, location: { pathname } });
			return appended;
		};

		it('uses the same auth-path rule as the Google Analytics gate', () => {
			expect(loader).toContain(AUTH_PATH.source);
		});

		it.each([
			'/login',
			'/login/github',
			'/LOGIN/google',
			'/register',
			'/desktop-auth/return',
			'/desktop-auth/telegram',
			'/password/forget',
			'/password/reset',
			'/email/verify',
			'/auth/link-confirm',
			'/workspaces/invitations/abc123',
		])('is not loaded on the auth page %s', (pathname) => {
			expect(injectedOn(pathname)).toHaveLength(0);
		});

		it.each([
			'/',
			'/tmgrdev/board',
			'/loginhistory',
			'/l/abc',
			'/settings/personas',
			'/workspaces',
		])('is loaded on %s', (pathname) => {
			const [script] = injectedOn(pathname);
			expect(script.src).toBe('https://analytics.dzik.email/js/script.js');
			expect(script.attrs['data-domain']).toBe('tmgr.dev');
		});
	});

	it.each([
		['/workspaces/invitations/abc123', true],
		['/login/github?code=x&state=y', true],
		['/password/reset?token=t', true],
		['/email/verify?token=t', true],
		['/login#token=t', true],
		['/login', false],
		['/register', false],
		['/workspaces/invitations', false],
		['/tmgrdev/board?x=1', false],
	])('carriesSecret(%s) is %s', (fullPath, expected) => {
		expect(carriesSecret(fullPath as string)).toBe(expected);
	});

	it('desktop build has no third-party analytics at all', () => {
		expect(stripAnalytics(html)).not.toMatch(
			/mc\.yandex|googletagmanager|analytics\.dzik|google-analytics/i,
		);
	});
});
