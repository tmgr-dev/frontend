import enAgents from '../locales/en/agents.json';
import ruAgents from '../locales/ru/agents.json';
import {
	buildNotifySnippets,
	resolveNotifyApiUrl,
} from './agentConnectSnippets';
import {
	buildNav,
	docPath,
	DOCS_LANGS,
	DOCS_ORIGIN,
	DOCS_UI,
	docsHead,
	loadDocs,
	prevNext,
	resolveDoc,
	youtubeEmbedUrl,
	type DocPage,
	type DocsHead,
	type DocsLang,
	type DocsNavSection,
} from './docs';
import { escapeHtml as e, renderDoc } from './docsMarkdown';
import { resolveMcpUrl } from './mcpConnectSnippets';

export interface DocsPrerenderOptions {
	files: Record<string, string>;
	template: string;
	apiBase?: string;
	headExtra?: string;
}

export interface DocsPrerenderResult {
	pages: { fileName: string; html: string }[];
	sitemap: string;
}

export const SITEMAP_EXTRA_PATHS = ['/privacy', '/stats'];

const AGENTS_TEXT = { en: enAgents, ru: ruAgents };

const componentMarkdown = (
	name: string,
	lang: DocsLang,
	notifyApiUrl: string | null,
): string => {
	const text = AGENTS_TEXT[lang];
	if (name === 'AiAgentsOverview') {
		const o = text.overview;
		const token = (value: string) => o.token.replace('{token}', value);
		return [
			`**${o.title}**`,
			'',
			`- \`tmgr\`: ${o.tmgrPurpose}. ${token(o.tmgrToken)}`,
			`- \`tmgr-notify\`: ${o.notifyPurpose}. ${token(o.notifyToken)}`,
		].join('\n');
	}
	if (name === 'AgentConnectTabs') {
		const tabs = text.connectTabs.tabs;
		const s = buildNotifySnippets({ apiUrl: notifyApiUrl });
		const block = (label: string, code: string, language: string) =>
			`**${label}**\n\n\`\`\`${language}\n${code}\n\`\`\``;
		return [
			block(tabs.claude, s.claudeCode, 'sh'),
			block(tabs.codex, s.codex, 'toml'),
			block(tabs.others, s.mcpJson, 'json'),
		].join('\n\n');
	}
	return '';
};

const navHtml = (nav: DocsNavSection[], lang: DocsLang, slug: string) =>
	`<div class="space-y-6 text-sm">${nav
		.map(
			(section) =>
				`<div><p class="mb-2 px-2 text-xs font-semibold tracking-wide text-ink-subtle uppercase">${e(
					section.title,
				)}</p><ul class="space-y-0.5">${section.items
					.map(
						(item) =>
							`<li><a href="${e(docPath(lang, item.slug))}"${
								item.slug === slug ? ' aria-current="page"' : ''
							} class="block rounded-md px-2 py-1.5 transition-colors focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-primary ${
								item.slug === slug
									? 'bg-muted font-medium text-ink'
									: 'text-ink-subtle hover:bg-muted hover:text-ink'
							}" data-testid="docs-nav-item">${e(item.title)}</a></li>`,
					)
					.join('')}</ul></div>`,
		)
		.join('')}</div>`;

const pageHtml = (
	pages: DocPage[],
	lang: DocsLang,
	slug: string,
	vars: { mcpUrl: string; notifyApiUrl: string | null },
): string => {
	const { page, fallback } = resolveDoc(pages, lang, slug)!;
	const ui = DOCS_UI[lang];
	const nav = buildNav(pages, lang);
	const pager = prevNext(nav, slug);
	const otherLang: DocsLang = lang === 'en' ? 'ru' : 'en';
	const rendered = renderDoc(page.body, {
		copyLabel: DOCS_UI[page.lang].copy,
		vars: { mcpUrl: vars.mcpUrl },
	});
	const content = rendered.segments
		.map((segment) => {
			if (segment.type === 'html')
				return `<div class="docs-prose">${segment.html}</div>`;
			const md = componentMarkdown(segment.name, page.lang, vars.notifyApiUrl);
			if (!md) return '';
			const html = renderDoc(md, { copyLabel: DOCS_UI[page.lang].copy })
				.segments.map((s) => (s.type === 'html' ? s.html : ''))
				.join('');
			return `<div class="my-6"><div class="docs-prose">${html}</div></div>`;
		})
		.join('');
	const toc = rendered.headings.filter((h) => h.depth === 2 || h.depth === 3);
	const video = youtubeEmbedUrl(page.video);
	const pagerLink = (target: DocPage, next: boolean) =>
		`<a href="${e(docPath(lang, target.slug))}" class="rounded-md border border-border px-4 py-3 hover:bg-muted${
			next ? ' text-right sm:col-start-2' : ''
		}" data-testid="docs-${next ? 'next' : 'prev'}"><span class="flex items-center ${
			next ? 'justify-end ' : ''
		}gap-1 text-xs text-ink-subtle">${e(
			next ? ui.next : ui.previous,
		)}</span><span class="mt-1 block text-sm font-medium text-ink">${e(
			target.title,
		)}</span></a>`;

	return `<div class="font-sans text-ink"><div class="flex min-h-screen"><div class="group/sidebar-wrapper flex min-h-svh w-full text-sidebar-foreground"><main class="app-canvas relative flex min-h-svh min-w-0 flex-1 flex-col bg-background pt-[var(--titlebar-h,0px)] pb-[var(--statusbar-h,0px)] max-sm:overflow-x-clip"><div class="flex min-h-max flex-1 flex-col gap-4 max-sm:overflow-x-clip">
<div class="mx-auto flex w-full max-w-7xl min-w-0 flex-1 gap-8 px-4 pt-4 pb-16 md:px-6 md:pt-6" data-testid="docs-page" data-docs-static>
<nav aria-label="${e(ui.docs)}" class="hidden w-56 shrink-0 lg:block"><div class="sticky top-[calc(var(--titlebar-h,0px)+1rem)] max-h-[calc(100svh-2rem)] overflow-y-auto pb-4"><a href="${e(
		docPath(lang, ''),
	)}" class="mb-4 block px-2 text-base font-semibold text-ink">${e(
		ui.docs,
	)}</a>${navHtml(nav, lang, slug)}</div></nav>
<article class="min-w-0 flex-1" lang="${page.lang}">
<div class="mb-6 flex items-center gap-2"><a href="${e(
		docPath(otherLang, slug),
	)}" hreflang="${otherLang}" lang="${otherLang}" aria-label="${e(
		ui.otherLanguageLabel,
	)}" class="ml-auto inline-flex h-9 shrink-0 items-center gap-1.5 rounded-md border border-input bg-background px-3 text-sm text-ink hover:bg-muted" data-testid="docs-lang-switch">${e(
		ui.otherLanguage,
	)}</a></div>
${
	fallback
		? `<p class="mb-6 rounded-md border border-border bg-muted px-3 py-2 text-sm text-ink-subtle" role="note" lang="${lang}" data-testid="docs-fallback-note">${e(
				ui.fallbackNote,
			)}</p>\n`
		: ''
}<header class="mb-8"><h1 tabindex="-1" class="text-2xl font-semibold tracking-tight text-ink outline-none sm:text-3xl" data-docs-title>${e(
		page.title,
	)}</h1>${
		page.description
			? `<p class="mt-2 text-base text-ink-subtle">${e(page.description)}</p>`
			: ''
	}</header>
${
	video
		? `<div class="mb-8 aspect-video overflow-hidden rounded-lg border border-border bg-muted" data-testid="docs-video"><iframe src="${e(
				video,
			)}" title="${e(`${ui.video}: ${page.title}`)}" class="h-full w-full" allow="accelerometer; clipboard-write; encrypted-media; gyroscope; picture-in-picture; web-share" referrerpolicy="strict-origin-when-cross-origin" loading="lazy" allowfullscreen></iframe></div>\n`
		: ''
}<div data-testid="docs-content">${content}</div>
${
	pager.prev || pager.next
		? `<nav aria-label="${e(
				`${ui.previous} / ${ui.next}`,
			)}" class="mt-12 grid gap-3 border-t border-border pt-6 sm:grid-cols-2">${
				pager.prev ? pagerLink(pager.prev, false) : ''
			}${pager.next ? pagerLink(pager.next, true) : ''}</nav>`
		: ''
}
</article>
${
	toc.length
		? `<aside aria-label="${e(
				ui.onThisPage,
			)}" class="hidden w-52 shrink-0 xl:block"><div class="sticky top-[calc(var(--titlebar-h,0px)+1rem)] max-h-[calc(100svh-2rem)] overflow-y-auto text-sm"><p class="mb-2 text-xs font-semibold tracking-wide text-ink-subtle uppercase">${e(
				ui.onThisPage,
			)}</p><ul class="space-y-1" data-testid="docs-toc">${toc
				.map(
					(h) =>
						`<li${h.depth === 3 ? ' class="pl-3"' : ''}><a href="#${e(
							h.id,
						)}" class="block py-0.5 text-ink-subtle hover:text-ink">${e(
							h.text,
						)}</a></li>`,
				)
				.join('')}</ul></div></aside>`
		: ''
}
</div>
</div></main></div></div></div>`;
};

export const headTagsHtml = (head: DocsHead): string =>
	head.tags
		.map(
			({ tag, attrs }) =>
				`<${tag} ${Object.entries(attrs)
					.map(([key, value]) => `${key}="${e(value)}"`)
					.join(' ')} data-docs-head>`,
		)
		.join('\n');

const replaceOnce = (html: string, pattern: RegExp, value: string) => {
	if (!pattern.test(html))
		throw new Error(`docs prerender: template has no ${pattern}`);
	return html.replace(pattern, () => value);
};

const fillTemplate = (
	template: string,
	head: DocsHead,
	headExtra: string,
	body: string,
) => {
	let html = replaceOnce(
		template,
		/<html\b[^>]*>/,
		`<html lang="${head.lang}">`,
	);
	html = replaceOnce(
		html,
		/<title>[^<]*<\/title>/,
		`<title>${e(head.title)}</title>\n${headTagsHtml(head)}`,
	);
	if (headExtra) html = replaceOnce(html, /<\/head>/, `${headExtra}\n</head>`);
	html = html.replace(/<noscript>[\s\S]*?<\/noscript>/, () => '');
	return replaceOnce(
		html,
		/<div id="app"><\/div>/,
		`<div id="app">${body}</div>`,
	);
};

const sitemapXml = (pages: DocPage[]): string => {
	const urls = pages
		.map((page) => docsHead(pages, page.lang, page.slug)!)
		.map(
			(head) =>
				`<url><loc>${e(head.canonical)}</loc>${head.alternates
					.map(
						(a) =>
							`<xhtml:link rel="alternate" hreflang="${e(
								a.hreflang,
							)}" href="${e(a.href)}"/>`,
					)
					.join('')}</url>`,
		);
	const extra = SITEMAP_EXTRA_PATHS.map(
		(path) => `<url><loc>${e(`${DOCS_ORIGIN}${path}`)}</loc></url>`,
	);
	return `<?xml version="1.0" encoding="UTF-8"?>
<urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9" xmlns:xhtml="http://www.w3.org/1999/xhtml">
${[...urls, ...extra].join('\n')}
</urlset>
`;
};

export function prerenderDocs(
	options: DocsPrerenderOptions,
): DocsPrerenderResult {
	const pages = loadDocs(options.files);
	const vars = {
		mcpUrl: resolveMcpUrl(options.apiBase, DOCS_ORIGIN),
		notifyApiUrl: resolveNotifyApiUrl(options.apiBase, DOCS_ORIGIN),
	};
	const slugs = [...new Set(pages.map((page) => page.slug))];
	const out = DOCS_LANGS.flatMap((lang) =>
		slugs.map((slug) => ({
			fileName: `${docPath(lang, slug).slice(1)}/index.html`,
			html: fillTemplate(
				options.template,
				docsHead(pages, lang, slug)!,
				options.headExtra ?? '',
				pageHtml(pages, lang, slug, vars),
			),
		})),
	);
	return { pages: out, sitemap: sitemapXml(pages) };
}
