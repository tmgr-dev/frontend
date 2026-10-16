import DOMPurify from 'dompurify';
import { Marked, type Tokens } from 'marked';
import { isFilenameAlt, parseImageTitle } from './imageTitle';
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
	image({ href, text, title }: Tokens.Image): string {
		const parsed = parseTmgrUrl(href);
		let source: string;
		if (parsed?.form === 'storage' && parsed.kind === 'file') {
			source = `data-tmgr-file="${parsed.id}" class="tmgr-image"`;
		} else if (/^\s*(javascript|data|vbscript|tmgr):/i.test(href)) {
			return '';
		} else {
			source = `src="${escapeAttribute(href)}"`;
		}
		const { layout, width } = parseImageTitle(title);
		const size = width ? ` width="${width}"` : '';
		const caption =
			text && !isFilenameAlt(text)
				? `<span class="page-image-caption">${escapeAttribute(text)}</span>`
				: '';
		return `<span class="page-image" data-layout="${
			layout ?? 'center'
		}"><img ${source} alt="${escapeAttribute(text)}"${size}>${caption}</span>`;
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
