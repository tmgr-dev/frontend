import { Marked, type Tokens } from 'marked';
import {
	COMPONENT_MARKER,
	headingSlug,
	markdownToPlain,
	splitHeadingId,
} from './docs';

export interface DocHeading {
	depth: number;
	text: string;
	id: string;
}

export type DocSegment =
	{ type: 'html'; html: string } | { type: 'component'; name: string };

export interface RenderedDoc {
	segments: DocSegment[];
	headings: DocHeading[];
}

interface RenderOptions {
	copyLabel: string;
	vars?: Record<string, string>;
}

type IdHeading = Tokens.Heading & { docsId?: string };

const escapeHtml = (value: string): string =>
	value
		.replace(/&/g, '&amp;')
		.replace(/</g, '&lt;')
		.replace(/>/g, '&gt;')
		.replace(/"/g, '&quot;');

export const safeUrl = (href: string | null | undefined): string | null => {
	const url = (href ?? '').replace(/[\x00-\x20]/g, '');
	return /^(https?:|mailto:|\/(?![/\\])|#)/i.test(url) ? url : null;
};

const hasFileExtension = (href: string): boolean =>
	/\.[A-Za-z0-9]+$/.test(href.split(/[?#]/)[0]);

export const DOCS_CODE_PRE_CLASS =
	'min-w-0 flex-1 overflow-x-auto whitespace-pre rounded-md border border-border bg-muted px-3 py-2 font-mono text-xs text-ink';

export const DOCS_COPY_BUTTON_CLASS =
	'inline-flex h-11 min-w-24 shrink-0 items-center justify-center self-end rounded-md border border-input bg-background px-2 text-xs font-medium shadow-xs transition-colors hover:bg-accent hover:text-accent-foreground sm:h-7 sm:self-start';

const createMarked = (copyLabel: string) => {
	const marked = new Marked({ gfm: true, breaks: false, async: false });
	marked.use({
		renderer: {
			heading(token) {
				const id = (token as IdHeading).docsId ?? '';
				const text = this.parser.parseInline(token.tokens);
				return `<h${token.depth} id="${escapeHtml(id)}" tabindex="-1">${text}</h${token.depth}>\n`;
			},
			link({ href, title, tokens }) {
				const text = this.parser.parseInline(tokens);
				const url = safeUrl(href);
				if (url === null) return text;
				const attrs = `href="${escapeHtml(url)}"${
					title ? ` title="${escapeHtml(title)}"` : ''
				}`;
				if (/^https?:/i.test(url))
					return `<a ${attrs} target="_blank" rel="noopener noreferrer">${text}</a>`;
				if (/^[/#]/.test(url) && !hasFileExtension(url))
					return `<a ${attrs} data-docs-link>${text}</a>`;
				if (url.startsWith('/'))
					return `<a ${attrs} target="_blank" rel="noopener">${text}</a>`;
				return `<a ${attrs}>${text}</a>`;
			},
			image({ href, title, text }) {
				const url = safeUrl(href);
				if (url === null || url.startsWith('mailto:')) return escapeHtml(text);
				return `<img src="${escapeHtml(url)}" alt="${escapeHtml(text)}"${
					title ? ` title="${escapeHtml(title)}"` : ''
				} loading="lazy">`;
			},
			code({ text, lang }) {
				const language = (lang || '').match(/^[\w-]+/)?.[0] ?? '';
				const langClass = language ? ` language-${escapeHtml(language)}` : '';
				return `<div class="docs-code flex flex-col gap-2 sm:flex-row sm:items-start"><pre class="${DOCS_CODE_PRE_CLASS}"><code class="docs-code-text${langClass}">${escapeHtml(
					text,
				)}</code></pre><button type="button" class="${DOCS_COPY_BUTTON_CLASS}" data-docs-copy data-label="${escapeHtml(
					copyLabel,
				)}">${escapeHtml(copyLabel)}</button></div>\n`;
			},
			html({ text, block }) {
				return block ? `<p>${escapeHtml(text)}</p>\n` : escapeHtml(text);
			},
		},
	});
	return marked;
};

export function renderDoc(body: string, options: RenderOptions): RenderedDoc {
	const vars = options.vars ?? {};
	const source = body.replace(/\{\{(\w+)\}\}/g, (all, name: string) =>
		name in vars ? vars[name] : all,
	);
	const marked = createMarked(options.copyLabel);
	const used = new Set<string>();
	const headings: DocHeading[] = [];
	const segments: DocSegment[] = [];
	let chunk: string[] = [];

	const flush = () => {
		const md = chunk.join('\n');
		chunk = [];
		if (!md.trim()) return;
		const tokens = marked.lexer(md);
		marked.walkTokens(tokens, (token) => {
			if (token.type !== 'heading') return;
			const heading = token as IdHeading;
			const { text, id } = splitHeadingId(heading.text);
			if (id) {
				heading.text = text;
				heading.tokens = marked.Lexer.lexInline(text, marked.defaults);
				used.add(id);
			}
			const plain = markdownToPlain(text);
			heading.docsId = id || headingSlug(plain, used);
			headings.push({ depth: heading.depth, text: plain, id: heading.docsId });
		});
		segments.push({ type: 'html', html: marked.parser(tokens) });
	};

	let fence = false;
	for (const line of source.replace(/\r\n/g, '\n').split('\n')) {
		if (/^\s*```/.test(line)) fence = !fence;
		const marker = !fence && line.trim().match(COMPONENT_MARKER);
		if (marker) {
			flush();
			segments.push({ type: 'component', name: marker[1] });
		} else chunk.push(line);
	}
	flush();
	return { segments, headings };
}
