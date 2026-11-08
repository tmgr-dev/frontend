import hljs from 'highlight.js/lib/core';
import bash from 'highlight.js/lib/languages/bash';
import json from 'highlight.js/lib/languages/json';
import yaml from 'highlight.js/lib/languages/yaml';
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

export const escapeHtml = (value: string): string =>
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

hljs.registerLanguage('bash', bash);
hljs.registerLanguage('json', json);
hljs.registerLanguage('yaml', yaml);
hljs.registerAliases(['shell'], { languageName: 'bash' });

// The markup of md-editor-v3's code block, so the library's own CSS styles it.
const codeBlockHtml = (
	text: string,
	language: string,
	copyLabel: string,
): string => {
	const highlighted =
		language && hljs.getLanguage(language)
			? hljs.highlight(text, { language, ignoreIllegals: true }).value
			: escapeHtml(text);
	const lines = '<span></span>'.repeat(text.split('\n').length);
	const lang = escapeHtml(language);
	return `<div class="docs-code"><div class="md-editor-code"><div class="md-editor-code-head"><div class="md-editor-code-flag"><span></span><span></span><span></span></div><div class="md-editor-code-action"><span class="md-editor-code-lang">${lang}</span><button type="button" class="md-editor-copy-button" data-docs-copy>${escapeHtml(
		copyLabel,
	)}</button></div></div><pre><code${
		language ? ` class="language-${lang}" language="${lang}"` : ''
	}><span class="md-editor-code-block">${highlighted}</span><span rn-wrapper aria-hidden="true">${lines}</span></code></pre></div></div>\n`;
};

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
				return codeBlockHtml(text, language, copyLabel);
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
