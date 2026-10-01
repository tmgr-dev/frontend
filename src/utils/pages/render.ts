import DOMPurify from 'dompurify';
import { Marked, type Tokens } from 'marked';
import { parseTmgrUrl } from './tmgrLinks';
import { PAGE_ALLOWED_URI_REGEXP } from './uri';

const escapeAttribute = (value: string): string =>
	value
		.replace(/&/g, '&amp;')
		.replace(/"/g, '&quot;')
		.replace(/</g, '&lt;')
		.replace(/>/g, '&gt;');

const renderer = {
	link(this: any, { href, title, tokens }: Tokens.Link): string {
		const text = this.parser.parseInline(tokens);
		const parsed = parseTmgrUrl(href);
		if (parsed) {
			return `<a href="#" class="tmgr-chip" data-tmgr="${escapeAttribute(
				href.trim(),
			)}">${text}</a>`;
		}
		if (/^\s*(javascript|data|vbscript|tmgr):/i.test(href)) return text;
		const titleAttr = title ? ` title="${escapeAttribute(title)}"` : '';
		return `<a href="${escapeAttribute(
			href,
		)}"${titleAttr} target="_blank" rel="noopener noreferrer">${text}</a>`;
	},
	image({ href, text }: Tokens.Image): string {
		const parsed = parseTmgrUrl(href);
		if (parsed?.form === 'storage' && parsed.kind === 'file') {
			return `<img data-tmgr-file="${parsed.id}" alt="${escapeAttribute(
				text,
			)}" class="tmgr-image">`;
		}
		if (/^\s*(javascript|data|vbscript|tmgr):/i.test(href)) return '';
		return `<img src="${escapeAttribute(href)}" alt="${escapeAttribute(
			text,
		)}">`;
	},
};

const marked = new Marked({ gfm: true, breaks: false, async: false, renderer });

export const renderPageMarkdown = (markdown: string): string =>
	markdown ? (marked.parse(markdown) as string) : '';

export const sanitizePageHtml = (html: string): string =>
	DOMPurify.sanitize(html, {
		ALLOWED_URI_REGEXP: PAGE_ALLOWED_URI_REGEXP,
		ADD_ATTR: ['target'],
	});
