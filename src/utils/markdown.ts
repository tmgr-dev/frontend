import { Marked } from 'marked';

/**
 * Markdown as it is written in comments and chat: GitHub flavour, and a single
 * newline is a line break, because people type comments like messages, not like
 * documents.
 *
 * The result is raw HTML — marked passes embedded HTML through untouched — so
 * every caller must sanitize it before it reaches the DOM (see sanitizeHtml).
 */
const marked = new Marked({
	gfm: true,
	breaks: true,
	async: false,
	renderer: {
		// Comment links point outward; keep the app open and deny the new page a
		// handle on the one that opened it.
		link({ href, title, tokens }) {
			const text = this.parser.parseInline(tokens);
			const titleAttr = title ? ` title="${title}"` : '';
			return `<a href="${href}"${titleAttr} target="_blank" rel="noopener noreferrer">${text}</a>`;
		},
	},
});

export const markdownToHtml = (text: string): string =>
	text ? (marked.parse(text) as string) : '';
