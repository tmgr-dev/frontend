import { APP_NAME, SEPARATOR } from '../composable/useDocumentTitle';

export type DocsLang = 'en' | 'ru';

export const DOCS_LANGS: DocsLang[] = ['en', 'ru'];

export const DOCS_COMPONENTS = [
	'AiAgentsOverview',
	'AgentConnectTabs',
] as const;

export interface DocPage {
	lang: DocsLang;
	slug: string;
	title: string;
	section: string;
	order: number;
	description: string;
	video: string;
	body: string;
}

export interface DocsNavSection {
	title: string;
	items: DocPage[];
}

export interface DocsSearchEntry {
	slug: string;
	pageTitle: string;
	heading: string;
	anchor: string;
	text: string;
}

export interface DocsSearchResult extends DocsSearchEntry {
	score: number;
	snippet: string;
}

export const DOCS_UI = {
	en: {
		docs: 'Docs',
		menu: 'Docs menu',
		search: 'Search docs',
		searchPlaceholder: 'Search the docs',
		noResults: 'No results',
		onThisPage: 'On this page',
		previous: 'Previous',
		next: 'Next',
		notFoundTitle: 'Page not found',
		notFoundText: 'There is no docs page at this address.',
		backToIndex: 'Go to the docs overview',
		fallbackNote:
			'This page is not available in English yet. Showing the Russian version.',
		otherLanguage: 'Русский',
		otherLanguageLabel: 'Читать на русском',
		copy: 'Copy',
		copied: 'Copied',
		copyFailed: 'Could not copy to clipboard',
		video: 'Video',
		close: 'Close',
	},
	ru: {
		docs: 'Документация',
		menu: 'Меню документации',
		search: 'Поиск по документации',
		searchPlaceholder: 'Искать в документации',
		noResults: 'Ничего не найдено',
		onThisPage: 'На этой странице',
		previous: 'Назад',
		next: 'Дальше',
		notFoundTitle: 'Страница не найдена',
		notFoundText: 'По этому адресу нет страницы документации.',
		backToIndex: 'Перейти к обзору документации',
		fallbackNote:
			'Этой страницы пока нет на русском, показана английская версия.',
		otherLanguage: 'English',
		otherLanguageLabel: 'Read in English',
		copy: 'Копировать',
		copied: 'Скопировано',
		copyFailed: 'Не удалось скопировать',
		video: 'Видео',
		close: 'Закрыть',
	},
} as const;

export function parseFrontMatter(raw: string): {
	data: Record<string, string>;
	body: string;
} {
	const text = raw.replace(/\r\n/g, '\n');
	const match = text.match(/^---\n([\s\S]*?)\n---(?:\n|$)/);
	if (!match) return { data: {}, body: text };
	const data: Record<string, string> = {};
	for (const line of match[1].split('\n')) {
		const at = line.indexOf(':');
		if (at > 0) data[line.slice(0, at).trim()] = line.slice(at + 1).trim();
	}
	return { data, body: text.slice(match[0].length) };
}

export function loadDocs(files: Record<string, string>): DocPage[] {
	const pages: DocPage[] = [];
	for (const [path, raw] of Object.entries(files)) {
		const match = path.match(/(?:^|\/)(en|ru)\/(.+)\.md$/);
		if (!match) continue;
		const { data, body } = parseFrontMatter(raw);
		const slug = match[2] === 'index' ? '' : match[2].replace(/\/index$/, '');
		const order = Number(data.order);
		pages.push({
			lang: match[1] as DocsLang,
			slug,
			title: data.title || slug,
			section: data.section || '',
			order: Number.isFinite(order) ? order : 1000,
			description: data.description || '',
			video: data.video || '',
			body,
		});
	}
	return pages;
}

export function docPath(lang: DocsLang, slug: string): string {
	return `/docs${lang === 'ru' ? '/ru' : ''}${slug ? `/${slug}` : ''}`;
}

export function parseDocsPath(path: string): { lang: DocsLang; slug: string } {
	const parts = path
		.replace(/^\/docs(?=\/|$)/, '')
		.split('/')
		.filter(Boolean);
	const lang: DocsLang = parts[0] === 'ru' ? 'ru' : 'en';
	return { lang, slug: (lang === 'ru' ? parts.slice(1) : parts).join('/') };
}

export function resolveDoc(
	pages: DocPage[],
	lang: DocsLang,
	slug: string,
): { page: DocPage; fallback: boolean } | null {
	const own = pages.find((p) => p.lang === lang && p.slug === slug);
	if (own) return { page: own, fallback: false };
	const other = pages.find((p) => p.slug === slug);
	return other ? { page: other, fallback: true } : null;
}

export const DOCS_ORIGIN = 'https://tmgr.dev';

export interface DocsHeadTag {
	tag: 'meta' | 'link';
	attrs: Record<string, string>;
}

export interface DocsHead {
	lang: DocsLang;
	title: string;
	canonical: string;
	alternates: { hreflang: string; href: string }[];
	tags: DocsHeadTag[];
}

export function docsHead(
	pages: DocPage[],
	lang: DocsLang,
	slug: string,
): DocsHead | null {
	const resolved = resolveDoc(pages, lang, slug);
	if (!resolved) return null;
	const { page } = resolved;
	const url = (l: DocsLang) => `${DOCS_ORIGIN}${docPath(l, slug)}`;
	const langs = DOCS_LANGS.filter((l) =>
		pages.some((p) => p.lang === l && p.slug === slug),
	);
	const canonical = url(page.lang);
	const alternates = [
		...langs.map((l) => ({ hreflang: l, href: url(l) })),
		{
			hreflang: 'x-default',
			href: url(langs.includes('en') ? 'en' : page.lang),
		},
	];
	const meta = (key: string, name: string, content: string): DocsHeadTag => ({
		tag: 'meta',
		attrs: { [key]: name, content },
	});
	const description = page.description
		? [
				meta('name', 'description', page.description),
				meta('property', 'og:description', page.description),
			]
		: [];
	return {
		lang,
		title: `${page.title}${SEPARATOR}${APP_NAME}`,
		canonical,
		alternates,
		tags: [
			...description,
			{ tag: 'link', attrs: { rel: 'canonical', href: canonical } },
			...alternates.map((a): DocsHeadTag => ({
				tag: 'link',
				attrs: { rel: 'alternate', hreflang: a.hreflang, href: a.href },
			})),
			meta('property', 'og:title', page.title),
			meta('property', 'og:url', canonical),
			meta('property', 'og:type', 'article'),
			meta('property', 'og:locale', page.lang === 'ru' ? 'ru_RU' : 'en_US'),
			meta('property', 'og:site_name', APP_NAME),
			meta('name', 'twitter:card', 'summary'),
		],
	};
}

const byOrder = (a: DocPage, b: DocPage) =>
	a.order - b.order || a.title.localeCompare(b.title);

export function buildNav(pages: DocPage[], lang: DocsLang): DocsNavSection[] {
	const slugs = [...new Set(pages.map((p) => p.slug))];
	const shown = slugs
		.map((slug) => resolveDoc(pages, lang, slug)!.page)
		.sort(byOrder);
	const sections: DocsNavSection[] = [];
	for (const page of shown) {
		const section = sections.find((s) => s.title === page.section);
		if (section) section.items.push(page);
		else sections.push({ title: page.section, items: [page] });
	}
	return sections;
}

export function prevNext(
	nav: DocsNavSection[],
	slug: string,
): { prev: DocPage | null; next: DocPage | null } {
	const flat = nav.flatMap((s) => s.items);
	const at = flat.findIndex((p) => p.slug === slug);
	if (at < 0) return { prev: null, next: null };
	return { prev: flat[at - 1] ?? null, next: flat[at + 1] ?? null };
}

export function youtubeEmbedUrl(id?: string | null): string | null {
	const value = (id ?? '').trim();
	return /^[A-Za-z0-9_-]{11}$/.test(value)
		? `https://www.youtube.com/embed/${value}`
		: null;
}

export type DocsVideo = { kind: 'youtube' | 'file'; src: string };

export function docsVideo(value?: string | null): DocsVideo | null {
	const youtube = youtubeEmbedUrl(value);
	if (youtube) return { kind: 'youtube', src: youtube };
	let url: URL;
	try {
		url = new URL((value ?? '').trim());
	} catch {
		return null;
	}
	return url.protocol === 'https:' &&
		url.origin === 'https://tmgr.dev' &&
		url.username === '' &&
		url.password === '' &&
		url.pathname.startsWith('/l/video/') &&
		url.pathname.endsWith('.mp4')
		? { kind: 'file', src: url.href }
		: null;
}

export const COMPONENT_MARKER = /^<!--\s*component:([A-Za-z0-9]+)\s*-->$/;

const HEADING_ID = /\s*\{#([A-Za-z0-9][\w-]*)\}\s*$/;

export function splitHeadingId(text: string): { text: string; id: string } {
	const match = text.match(HEADING_ID);
	return match
		? { text: text.replace(HEADING_ID, ''), id: match[1] }
		: { text, id: '' };
}

export function headingSlug(text: string, used: Set<string>): string {
	const base =
		text
			.toLowerCase()
			.replace(/[^\p{L}\p{N}\s-]/gu, '')
			.trim()
			.replace(/\s+/g, '-') || 'section';
	let id = base;
	for (let n = 1; used.has(id); n++) id = `${base}-${n}`;
	used.add(id);
	return id;
}

export function markdownToPlain(md: string): string {
	return md
		.split('\n')
		.filter((line) => !COMPONENT_MARKER.test(line.trim()))
		.join('\n')
		.replace(/^```.*$/gm, '')
		.replace(/\{\{\w+\}\}/g, '')
		.replace(/!?\[([^\]]*)\]\([^)]*\)/g, '$1')
		.replace(/[`*_>#|]/g, '')
		.replace(/\s+/g, ' ')
		.trim();
}

export function buildSearchIndex(pages: DocPage[]): DocsSearchEntry[] {
	const entries: DocsSearchEntry[] = [];
	for (const page of pages) {
		const used = new Set<string>();
		let current: DocsSearchEntry = {
			slug: page.slug,
			pageTitle: page.title,
			heading: '',
			anchor: '',
			text: page.description,
		};
		const lines: string[] = [];
		const flush = () => {
			current.text = markdownToPlain(`${current.text}\n${lines.join('\n')}`);
			entries.push(current);
			lines.length = 0;
		};
		let fence = false;
		for (const line of page.body.split('\n')) {
			if (/^\s*```/.test(line)) fence = !fence;
			const heading = !fence && line.match(/^#{2,3}\s+(.+)$/);
			if (!heading) {
				lines.push(line);
				continue;
			}
			flush();
			const { text, id } = splitHeadingId(heading[1]);
			const plain = markdownToPlain(text);
			if (id) used.add(id);
			current = {
				slug: page.slug,
				pageTitle: page.title,
				heading: plain,
				anchor: id || headingSlug(plain, used),
				text: '',
			};
		}
		flush();
	}
	return entries;
}

const snippetAround = (text: string, term: string): string => {
	const at = text.toLowerCase().indexOf(term);
	if (at < 0) return text.slice(0, 120);
	const start = Math.max(0, at - 40);
	return `${start > 0 ? '…' : ''}${text.slice(start, start + 120)}${
		start + 120 < text.length ? '…' : ''
	}`;
};

export function searchDocs(
	entries: DocsSearchEntry[],
	query: string,
	limit = 20,
): DocsSearchResult[] {
	const terms = query.toLowerCase().split(/\s+/).filter(Boolean);
	if (!terms.length) return [];
	const has = (value: string) => {
		const lower = value.toLowerCase();
		return terms.every((t) => lower.includes(t));
	};
	const results: DocsSearchResult[] = [];
	entries.forEach((entry) => {
		let score = 0;
		if (!entry.heading && has(entry.pageTitle)) score = 3;
		else if (entry.heading && has(entry.heading)) score = 2;
		else if (has(`${entry.pageTitle} ${entry.heading} ${entry.text}`))
			score = 1;
		if (score)
			results.push({
				...entry,
				score,
				snippet: snippetAround(entry.text, terms[0]),
			});
	});
	return results
		.map((r, i) => ({ r, i }))
		.sort((a, b) => b.r.score - a.r.score || a.i - b.i)
		.slice(0, limit)
		.map(({ r }) => r);
}
