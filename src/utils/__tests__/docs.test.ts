import { readdirSync, readFileSync, statSync } from 'fs';
import { join, relative } from 'path';
import {
	buildNav,
	buildSearchIndex,
	COMPONENT_MARKER,
	docPath,
	DOCS_COMPONENTS,
	DOCS_UI,
	loadDocs,
	parseDocsPath,
	parseFrontMatter,
	prevNext,
	resolveDoc,
	searchDocs,
	youtubeEmbedUrl,
	type DocsLang,
} from '../docs';
import { renderDoc } from '../docsMarkdown';

const docsDir = join(__dirname, '../../docs');

const walk = (dir: string): string[] =>
	readdirSync(dir).flatMap((name) => {
		const full = join(dir, name);
		return statSync(full).isDirectory() ? walk(full) : [full];
	});

const realFiles: Record<string, string> = Object.fromEntries(
	walk(docsDir)
		.filter((f) => f.endsWith('.md'))
		.map((f) => [`./${relative(docsDir, f)}`, readFileSync(f, 'utf8')]),
);
const realPages = loadDocs(realFiles);

const page = (front: Record<string, string | number>, body = '') =>
	`---\n${Object.entries(front)
		.map(([k, v]) => `${k}: ${v}`)
		.join('\n')}\n---\n${body}`;

const fixture = loadDocs({
	'./en/index.md': page({ title: 'Overview', section: 'Start', order: 0 }),
	'./en/cli.md': page(
		{ title: 'Command line', section: 'Guides', order: 20 },
		'Intro about the terminal.\n\n## Install {#install}\n\nRun the board script.\n\n## Board keys\n\nPress L to move.',
	),
	'./en/agents.md': page(
		{ title: 'AI agents', section: 'Guides', order: 10 },
		'## Board of agents {#board}\n\nAgents text.',
	),
	'./en/cli/one.md': page({
		title: 'Lesson one',
		section: 'Tutorials',
		order: 30,
	}),
	'./ru/index.md': page({ title: 'Обзор', section: 'Начало', order: 0 }),
	'./ru/cli.md': page({
		title: 'Командная строка',
		section: 'Руководства',
		order: 20,
	}),
	'./notes/readme.md': page({ title: 'Ignored', section: 'X' }),
});

describe('docs front-matter and index', () => {
	it('parses key: value front-matter and keeps the body', () => {
		expect(
			parseFrontMatter('---\ntitle: A: b\nvideo:\norder: 3\n---\n## Hi\n'),
		).toEqual({
			data: { title: 'A: b', video: '', order: '3' },
			body: '## Hi\n',
		});
		expect(parseFrontMatter('no front matter')).toEqual({
			data: {},
			body: 'no front matter',
		});
		expect(parseFrontMatter('---\r\ntitle: X\r\n---\r\nbody').data.title).toBe(
			'X',
		);
	});

	it('maps files to language and slug, ignoring other folders', () => {
		expect(fixture.map((p) => `${p.lang}:${p.slug}`).sort()).toEqual([
			'en:',
			'en:agents',
			'en:cli',
			'en:cli/one',
			'ru:',
			'ru:cli',
		]);
	});

	it('groups the sidebar by section, ordered by the lowest page order', () => {
		const nav = buildNav(fixture, 'en');
		expect(nav.map((s) => [s.title, s.items.map((p) => p.slug)])).toEqual([
			['Start', ['']],
			['Guides', ['agents', 'cli']],
			['Tutorials', ['cli/one']],
		]);
	});

	it('fills a missing page with the other language in the sidebar', () => {
		const nav = buildNav(fixture, 'ru');
		expect(
			nav.flatMap((s) => s.items.map((p) => `${p.lang}:${p.slug}`)),
		).toEqual(['ru:', 'en:agents', 'ru:cli', 'en:cli/one']);
	});

	it('follows sidebar order for previous and next', () => {
		const nav = buildNav(fixture, 'en');
		const at = (slug: string) => {
			const { prev, next } = prevNext(nav, slug);
			return [prev?.slug ?? null, next?.slug ?? null];
		};
		expect(at('')).toEqual([null, 'agents']);
		expect(at('agents')).toEqual(['', 'cli']);
		expect(at('cli/one')).toEqual(['cli', null]);
		expect(at('nope')).toEqual([null, null]);
	});

	it('resolves slugs, falls back to the other language and reports not found', () => {
		expect(resolveDoc(fixture, 'en', 'cli')).toMatchObject({
			page: { lang: 'en', title: 'Command line' },
			fallback: false,
		});
		expect(resolveDoc(fixture, 'ru', 'agents')).toMatchObject({
			page: { lang: 'en', slug: 'agents' },
			fallback: true,
		});
		expect(resolveDoc(fixture, 'en', 'nope')).toBeNull();
		expect(resolveDoc(fixture, 'ru', 'nope')).toBeNull();
	});

	it('maps URLs to language and slug and back', () => {
		expect(parseDocsPath('/docs')).toEqual({ lang: 'en', slug: '' });
		expect(parseDocsPath('/docs/')).toEqual({ lang: 'en', slug: '' });
		expect(parseDocsPath('/docs/agents')).toEqual({
			lang: 'en',
			slug: 'agents',
		});
		expect(parseDocsPath('/docs/ru')).toEqual({ lang: 'ru', slug: '' });
		expect(parseDocsPath('/docs/ru/cli/install-and-sign-in')).toEqual({
			lang: 'ru',
			slug: 'cli/install-and-sign-in',
		});
		expect(docPath('en', '')).toBe('/docs');
		expect(docPath('ru', '')).toBe('/docs/ru');
		expect(docPath('ru', 'cli')).toBe('/docs/ru/cli');
	});
});

describe('docs search', () => {
	const entries = buildSearchIndex(
		buildNav(fixture, 'en').flatMap((s) => s.items),
	);

	it('ranks title over heading over body', () => {
		const results = searchDocs(entries, 'board');
		expect(results.map((r) => [r.slug, r.anchor, r.score])).toEqual([
			['agents', 'board', 2],
			['cli', 'board-keys', 2],
			['cli', 'install', 1],
		]);
		expect(searchDocs(entries, 'command')[0]).toMatchObject({
			slug: 'cli',
			anchor: '',
			score: 3,
		});
	});

	it('needs every word and ignores case', () => {
		expect(searchDocs(entries, 'BOARD script').map((r) => r.anchor)).toEqual([
			'install',
		]);
	});

	it('returns nothing for an empty query or no match', () => {
		expect(searchDocs(entries, '   ')).toEqual([]);
		expect(searchDocs(entries, 'zzzz')).toEqual([]);
		expect(DOCS_UI.en.noResults).toBe('No results');
		expect(DOCS_UI.ru.noResults).toBe('Ничего не найдено');
	});
});

describe('docs rendering', () => {
	const render = (md: string, vars?: Record<string, string>) =>
		renderDoc(md, { copyLabel: 'Copy', vars });
	const html = (md: string) =>
		render(md)
			.segments.map((s) => (s.type === 'html' ? s.html : `[${s.name}]`))
			.join('');

	it('uses {#id} heading ids and auto slugs', () => {
		const doc = render(
			'## Links {#links}\n\n### Plain `code` heading\n\n## Links',
		);
		expect(doc.headings).toEqual([
			{ depth: 2, text: 'Links', id: 'links' },
			{ depth: 3, text: 'Plain code heading', id: 'plain-code-heading' },
			{ depth: 2, text: 'Links', id: 'links-1' },
		]);
		expect(html('## Links {#links}')).toContain(
			'<h2 id="links" tabindex="-1">Links</h2>',
		);
	});

	it('escapes raw HTML and keeps code text intact', () => {
		const out = html(
			'<script>alert(1)</script>\n\nText <b>bold</b>\n\n```sh\na < b && c\n```',
		);
		expect(out).not.toMatch(/<script|<b>/);
		expect(out).toContain('&lt;script&gt;');
		expect(out).toContain('a &lt; b &amp;&amp; c');
		expect(out).toContain('data-docs-copy');
	});

	it('routes internal links and opens files and external links in a new tab', () => {
		const out = html(
			'[a](/docs/cli) [b](#safety) [c](/docs/plugins.md) [d](https://example.com) [e](javascript:alert(1))',
		);
		expect(out).toContain('<a href="/docs/cli" data-docs-link>a</a>');
		expect(out).toContain('<a href="#safety" data-docs-link>b</a>');
		expect(out).toContain(
			'<a href="/docs/plugins.md" target="_blank" rel="noopener">c</a>',
		);
		expect(out).toContain(
			'<a href="https://example.com" target="_blank" rel="noopener noreferrer">d</a>',
		);
		expect(out).not.toContain('javascript:');
	});

	it.each([
		['[x](<java\tscript:alert(1)>)'],
		['[x](<java\nscript:alert(1)>)'],
		['[x](&#106;avascript:alert(1))'],
		['[x](JaVaScRiPt:alert(1))'],
		['[x](data:text/html,hi)'],
		['[x](vbscript:msgbox)'],
		['![x](javascript:alert(1))'],
		['![x](<java\tscript:alert(1)>)'],
		['[x](//evil.example.com/path)'],
		['[x](/\\evil.example.com)'],
		['[x](relative/path)'],
	])('drops unsafe or ambiguous URL %s', (md) => {
		const out = html(md);
		expect(out).not.toMatch(/href=|src=/);
		expect(out).not.toMatch(/javascript:/i);
		expect(out).not.toContain('data-docs-link');
	});

	it('keeps allowed URLs on links and images', () => {
		expect(html('[m](mailto:you@example.com)')).toContain(
			'<a href="mailto:you@example.com">m</a>',
		);
		expect(html('![logo](/img/logo.png)')).toContain(
			'<img src="/img/logo.png" alt="logo"',
		);
		expect(html('![pic](https://example.com/a.png)')).toContain(
			'src="https://example.com/a.png"',
		);
	});

	it('splits on component markers and fills runtime values', () => {
		const doc = render(
			'Before {{mcpUrl}}\n\n<!-- component:AgentConnectTabs -->\n\nAfter {{unknown}}',
			{ mcpUrl: 'https://example.com/mcp' },
		);
		expect(doc.segments.map((s) => s.type)).toEqual([
			'html',
			'component',
			'html',
		]);
		expect(doc.segments[1]).toEqual({
			type: 'component',
			name: 'AgentConnectTabs',
		});
		expect(
			html('```md\n<!-- component:AgentConnectTabs -->\n```'),
		).not.toContain('[AgentConnectTabs]');
		const text = doc.segments
			.map((s) => (s.type === 'html' ? s.html : ''))
			.join('');
		expect(text).toContain('Before <a href="https://example.com/mcp"');
		expect(text).toContain('After {{unknown}}');
	});
});

describe('youtube player', () => {
	it('embeds only a valid 11-character id', () => {
		expect(youtubeEmbedUrl('dQw4w9WgXcQ')).toBe(
			'https://www.youtube.com/embed/dQw4w9WgXcQ',
		);
		expect(youtubeEmbedUrl(' a-b_c1234XY ')).toBe(
			'https://www.youtube.com/embed/a-b_c1234XY',
		);
		expect(youtubeEmbedUrl('short')).toBeNull();
		expect(youtubeEmbedUrl('dQw4w9WgXcQ"><x')).toBeNull();
		expect(youtubeEmbedUrl('')).toBeNull();
		expect(youtubeEmbedUrl(undefined)).toBeNull();
	});
});

describe('docs content', () => {
	const REQUIRED: Record<string, string[]> = {
		cli: [
			'tasks',
			'runner',
			'decisions',
			'phone',
			'autopilot',
			'usage',
			'desktop',
			'agents',
			'safety',
			'more',
		],
		agents: [
			'get-a-token',
			'connect-your-agent',
			'task-mcp',
			'alarms',
			'troubleshooting',
			'links',
			'plugins',
		],
	};
	const langs: DocsLang[] = ['en', 'ru'];
	const SLUGS = [
		'',
		'agents',
		'cli',
		'cli/install-and-sign-in',
		'cli/tasks-from-the-terminal',
	];

	it('has every page in both languages with title and section', () => {
		for (const lang of langs)
			for (const slug of SLUGS) {
				const found = realPages.find((p) => p.lang === lang && p.slug === slug);
				expect(found).toBeDefined();
				expect(found!.title).not.toBe(slug);
				expect(found!.section).not.toBe('');
			}
		expect(
			realPages.some((p) => ['plugins.md', 'plugin-sdk'].includes(p.slug)),
		).toBe(false);
	});

	it.each(langs)(
		'keeps the old anchor ids on the %s cli and agents pages',
		(lang) => {
			for (const [slug, ids] of Object.entries(REQUIRED)) {
				const doc = realPages.find((p) => p.lang === lang && p.slug === slug)!;
				const rendered = renderDoc(doc.body, {
					copyLabel: 'Copy',
					vars: { mcpUrl: 'https://example.com/mcp' },
				});
				const html = rendered.segments
					.map((s) => (s.type === 'html' ? s.html : ''))
					.join('');
				for (const id of ids) expect(html).toContain(`id="${id}"`);
			}
		},
	);

	it('uses the same heading ids in both languages', () => {
		for (const slug of SLUGS) {
			const ids = langs.map((lang) => {
				const doc = realPages.find((p) => p.lang === lang && p.slug === slug)!;
				return renderDoc(doc.body, { copyLabel: 'Copy' }).headings.map(
					(h) => h.id,
				);
			});
			expect(ids[1]).toEqual(ids[0]);
		}
	});

	it('only embeds whitelisted components', () => {
		const names = realPages.flatMap((p) =>
			p.body
				.split('\n')
				.map((l) => l.trim().match(COMPONENT_MARKER)?.[1])
				.filter(Boolean),
		);
		expect(names.length).toBeGreaterThan(0);
		for (const name of names) expect(DOCS_COMPONENTS).toContain(name);
	});

	it('leaves the tutorial video empty or valid', () => {
		for (const p of realPages.filter((p) => p.slug.startsWith('cli/'))) {
			expect(p.video === '' || youtubeEmbedUrl(p.video) !== null).toBe(true);
		}
	});

	it('contains no personal data or tokens', () => {
		for (const [path, text] of Object.entries(realFiles)) {
			expect([path, text.match(/\/Users\/|\/home\//g)]).toEqual([path, null]);
			const emails = (text.match(/[\w.+-]+@[\w-]+(?:\.[\w-]+)+/g) ?? []).filter(
				(e) => !/@example\.com$/i.test(e),
			);
			expect([path, emails]).toEqual([path, []]);
			expect([path, text.match(/tmgrn_[A-Za-z0-9]{4,}/g)]).toEqual([
				path,
				null,
			]);
		}
	});
});
