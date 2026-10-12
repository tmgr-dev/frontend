import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import { stripAnalytics } from '../stripAnalytics';

const html = readFileSync(join(process.cwd(), 'index.html'), 'utf8');

describe('index.html analytics', () => {
	it('web build loads neither Yandex nor Google tags statically', () => {
		expect(html).not.toMatch(/mc\.yandex|metrika|ym\(|googletagmanager|gtag\(/i);
	});

	it('web build keeps the self-hosted analytics inside the strip markers', () => {
		expect(html).toContain('analytics.dzik');
		expect(stripAnalytics(html)).not.toContain('analytics.dzik');
	});

	it('desktop build has no third-party analytics at all', () => {
		expect(stripAnalytics(html)).not.toMatch(
			/mc\.yandex|googletagmanager|analytics\.dzik|google-analytics/i,
		);
	});
});
