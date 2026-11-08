import { readdirSync, readFileSync } from 'fs';
import { join } from 'path';
import { docPath, DOCS_ORIGIN, loadDocs } from '../docs';
import { prerenderDocs } from '../docsPrerender';

const root = join(__dirname, '../../..');
const docsDir = join(root, 'src/docs');
const files: Record<string, string> = Object.fromEntries(
	readdirSync(docsDir, { recursive: true, encoding: 'utf8' })
		.filter((f) => f.endsWith('.md'))
		.map((f) => [`./${f}`, readFileSync(join(docsDir, f), 'utf8')]),
);
const template = readFileSync(join(root, 'index.html'), 'utf8');
const pages = loadDocs(files);
const API = 'https://api.example.com/api/';
const result = prerenderDocs({
	files,
	template,
	apiBase: API,
	headExtra: '<link rel="modulepreload" href="/assets/js/DocsPage-x.js">',
});
const byFile = new Map(result.pages.map((p) => [p.fileName, p.html]));
const fileFor = (lang: 'en' | 'ru', slug: string) =>
	`${docPath(lang, slug).slice(1)}/index.html`;
const attr = (html: string, pattern: RegExp) => html.match(pattern)?.[1];

describe('docs prerender over the real docs', () => {
	it('emits one page per md file and language, nothing else', () => {
		expect(pages.length).toBe(26);
		expect([...byFile.keys()].sort()).toEqual(
			pages.map((p) => fileFor(p.lang, p.slug)).sort(),
		);
		expect(byFile.has('docs/index.html')).toBe(true);
		expect(byFile.has('docs/ru/index.html')).toBe(true);
		const titles = [...byFile.values()].map(
			(html) => html.match(/<title>([^<]*)<\/title>/)?.[1],
		);
		expect(new Set(titles).size).toBe(pages.length);
	});

	it.each(pages.map((p) => [`${p.lang}:${p.slug || '/'}`, p] as const))(
		'%s has head tags, lang and static content',
		(_, page) => {
			const html = byFile.get(fileFor(page.lang, page.slug))!;
			const url = `${DOCS_ORIGIN}${docPath(page.lang, page.slug)}`;
			expect(page.description).not.toBe('');
			expect(html).toMatch(
				new RegExp(`^<!doctype html>\\s*<html lang="${page.lang}">`),
			);
			expect(attr(html, /<title>([^<]*)<\/title>/)).toBe(
				`${page.title} | TMGR`,
			);
			expect(html.match(/<title>/g)).toHaveLength(1);
			expect(attr(html, /<meta name="description" content="([^"]*)"/)).toBe(
				page.description.replace(/"/g, '&quot;'),
			);
			expect(attr(html, /<link rel="canonical" href="([^"]*)"/)).toBe(url);
			expect(url.endsWith('/')).toBe(false);
			for (const lang of ['en', 'ru'])
				expect(html).toContain(
					`<link rel="alternate" hreflang="${lang}" href="${DOCS_ORIGIN}${docPath(
						lang as 'en' | 'ru',
						page.slug,
					)}" data-docs-head>`,
				);
			expect(html).toContain(
				`hreflang="x-default" href="${DOCS_ORIGIN}${docPath('en', page.slug)}"`,
			);
			for (const prop of [
				'og:title',
				'og:description',
				'og:url',
				'og:type',
				'og:locale',
				'og:site_name',
			])
				expect(html).toContain(`<meta property="${prop}" content="`);
			expect(html).toContain('<meta name="twitter:card" content="summary"');
			expect(html).toContain(`<meta property="og:url" content="${url}"`);
			expect(html).not.toContain('og:image');
			expect(html).toContain(
				'<link rel="modulepreload" href="/assets/js/DocsPage-x.js">\n</head>',
			);
			expect(html).not.toContain('<noscript>');
			expect(html).toContain('data-docs-title>');
			expect(html).toContain(
				`<article class="min-w-0 flex-1" lang="${page.lang}">`,
			);
			const navLinks = html.match(/data-testid="docs-nav-item"/g) ?? [];
			expect(navLinks).toHaveLength(13);
			expect(html).toContain(
				`<a href="${docPath(page.lang, page.slug)}" aria-current="page"`,
			);
			expect(html).not.toMatch(/\{\{|<!--\s*component:/);
			expect(html).toContain('<div class="docs-prose">');
		},
	);

	it('renders headings with ids, code blocks and resolves {{mcpUrl}}', () => {
		const html = byFile.get('docs/agents/index.html')!;
		expect(html).toContain('<h2 id="get-a-token" tabindex="-1">');
		expect(html).toContain('<a href="#task-mcp" class="block py-0.5');
		expect(html).toContain('<div class="md-editor-code-head">');
		expect(html).toContain('<code class="language-sh" language="sh">');
		expect(html).toContain('<span rn-wrapper aria-hidden="true"><span></span>');
		expect(html).toContain(
			'<button type="button" class="md-editor-copy-button" data-docs-copy>',
		);
		expect(html).toMatch(/<span class="hljs-/);
		expect(html).toContain('https://api.example.com/mcp');
		expect(html).toContain('tmgr-notify');
		expect(html).toContain('TMGR_URL=https://api.example.com');
		expect(html).toContain('data-testid="docs-next"');
		expect(byFile.get('docs/ru/agents/index.html')).toContain(
			'Работа с задачами и временем',
		);
	});

	it('keeps the template head and body scripts', () => {
		const html = byFile.get('docs/cli/index.html')!;
		expect(html).toContain(
			'<script type="module" src="/src/main.ts"></script>',
		);
		expect(html).toContain("localStorage.getItem('theme')");
		expect(html).toContain('<div id="app"><div class="font-sans text-ink">');
	});
});

describe('sitemap', () => {
	const xml = result.sitemap;

	it('lists every docs URL with alternates plus privacy and stats', () => {
		expect(xml.startsWith('<?xml version="1.0" encoding="UTF-8"?>')).toBe(true);
		expect(xml).toContain('xmlns:xhtml="http://www.w3.org/1999/xhtml"');
		const locs = [...xml.matchAll(/<loc>([^<]*)<\/loc>/g)].map((m) => m[1]);
		expect(locs.sort()).toEqual(
			[
				...pages.map((p) => `${DOCS_ORIGIN}${docPath(p.lang, p.slug)}`),
				`${DOCS_ORIGIN}/privacy`,
				`${DOCS_ORIGIN}/stats`,
			].sort(),
		);
		expect(xml.match(/hreflang="x-default"/g)).toHaveLength(pages.length);
		expect(xml).toContain(
			`<url><loc>${DOCS_ORIGIN}/docs/ru/cli</loc><xhtml:link rel="alternate" hreflang="en" href="${DOCS_ORIGIN}/docs/cli"/>`,
		);
		expect(xml.match(/<url>/g)).toHaveLength(xml.match(/<\/url>/g)!.length);
	});
});

describe('llms.txt', () => {
	it('lists every docs page as an absolute link', () => {
		const llms = readFileSync(join(root, 'public/llms.txt'), 'utf8');
		for (const p of pages)
			expect(llms).toContain(`](${DOCS_ORIGIN}${docPath(p.lang, p.slug)})`);
	});
});

describe('slug missing in one language', () => {
	const fx = (front: string, body = 'Text.') => `---\n${front}\n---\n${body}`;
	const out = prerenderDocs({
		files: {
			'./en/index.md': fx('title: Overview\ndescription: Start'),
			'./ru/index.md': fx('title: Обзор\ndescription: Начало'),
			'./en/only.md': fx('title: Only English\ndescription: Just EN & more'),
		},
		template,
	});
	const html = out.pages.find(
		(p) => p.fileName === 'docs/ru/only/index.html',
	)!.html;

	it('prerenders the fallback with canonical to the existing language', () => {
		expect(html).toContain('<html lang="ru">');
		expect(html).toContain(
			`<link rel="canonical" href="${DOCS_ORIGIN}/docs/only" data-docs-head>`,
		);
		expect(html).not.toContain('hreflang="ru" href=');
		expect(html).toContain('data-testid="docs-fallback-note"');
		expect(html).toContain('content="Just EN &amp; more"');
		expect(html).toContain('<article class="min-w-0 flex-1" lang="en">');
	});

	it('leaves the fallback URL out of the sitemap', () => {
		expect(out.sitemap).toContain(`<loc>${DOCS_ORIGIN}/docs/only</loc>`);
		expect(out.sitemap).not.toContain(`<loc>${DOCS_ORIGIN}/docs/ru/only</loc>`);
	});

	it('fails loudly on a template without the app root', () => {
		expect(() =>
			prerenderDocs({ files: {}, template: '<html><title>x</title></html>' }),
		).not.toThrow();
		expect(() =>
			prerenderDocs({
				files: { './en/index.md': fx('title: A') },
				template: '<html lang="en"><title>TMGR</title></html>',
			}),
		).toThrow(/app/);
	});
});
