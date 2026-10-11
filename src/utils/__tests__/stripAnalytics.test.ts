import { stripAnalytics } from '../stripAnalytics';

describe('stripAnalytics', () => {
	it('removes every marked block and keeps the rest', () => {
		const html = [
			'<head>',
			'<!-- analytics:start -->',
			'<script src="https://www.googletagmanager.com/gtag/js"></script>',
			'<!-- analytics:end -->',
			'<title>TMGR</title>',
			'</head>',
			'<body>',
			'<!-- analytics:start -->',
			'<noscript><img src="https://mc.yandex.ru/watch/1"></noscript>',
			'<!-- analytics:end -->',
			'</body>',
		].join('\n');

		const result = stripAnalytics(html);

		expect(result).not.toMatch(/googletagmanager|yandex|analytics:/);
		expect(result).toContain('<title>TMGR</title>');
		expect(result).toContain('<body>');
	});

	it('leaves html without markers untouched', () => {
		expect(stripAnalytics('<p>x</p>')).toBe('<p>x</p>');
	});
});
