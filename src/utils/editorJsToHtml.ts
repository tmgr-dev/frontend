import { listStart, normalizeBlocks, type ListItem } from '@/utils/editor';
import { isLegacyHtml } from '@/utils/editorType';
import DOMPurify from 'dompurify';

interface Block {
	type?: string;
	data?: any;
}

const escapeHtml = (value: unknown): string =>
	String(value ?? '')
		.replace(/&/g, '&amp;')
		.replace(/</g, '&lt;')
		.replace(/>/g, '&gt;')
		.replace(/"/g, '&quot;')
		.replace(/'/g, '&#39;');

const html = (value: unknown): string =>
	typeof value === 'string' ? value : '';

// @bomdi/codebox stores the highlighted editor innerHTML; an inert parser recovers the text.
const codeboxText = (value: unknown): string =>
	new DOMParser().parseFromString(
		html(value)
			.replace(/<br\s*\/?>/gi, '\n')
			.replace(/<\/div>\s*<div[^>]*>/gi, '\n'),
		'text/html',
	).body.textContent ?? '';

const caption = (value: unknown): string =>
	html(value) ? `<figcaption>${html(value)}</figcaption>` : '';

const renderList = (raw: unknown, style: string, start = 1): string => {
	const items: Partial<ListItem>[] = (Array.isArray(raw) ? raw : []).filter(
		(item) => item && typeof item === 'object',
	);
	if (items.length === 0) return '';
	if (style === 'checklist')
		return `<ul class="checklist">${items
			.map(
				(item) =>
					`<li><input type="checkbox" disabled${
						item.meta?.checked ? ' checked' : ''
					}> <div>${html(item.content)}${renderList(
						item.items,
						style,
					)}</div></li>`,
			)
			.join('')}</ul>`;
	const tag = style === 'ordered' ? 'ol' : 'ul';
	const lis = items
		.map(
			(item) =>
				`<li>${html(item.content)}${renderList(item.items, style)}</li>`,
		)
		.join('');
	return `<${tag}${start > 1 ? ` start="${start}"` : ''}>${lis}</${tag}>`;
};

const renderTable = (data: any): string => {
	const rows: unknown[] = Array.isArray(data?.content) ? data.content : [];
	if (rows.length === 0) return '';
	const row = (cells: unknown, tag: 'th' | 'td') =>
		`<tr>${(Array.isArray(cells) ? cells : [])
			.map((cell) => `<${tag}>${html(cell)}</${tag}>`)
			.join('')}</tr>`;
	const [first, ...rest] = rows;
	const head = data?.withHeadings ? `<thead>${row(first, 'th')}</thead>` : '';
	const body = (data?.withHeadings ? rest : rows)
		.map((cells) => row(cells, 'td'))
		.join('');
	return `<table>${head}<tbody>${body}</tbody></table>`;
};

const renderBlock = ({ type, data = {} }: Block): string => {
	switch (type) {
		case 'header': {
			const level = Math.min(
				6,
				Math.max(1, Math.trunc(Number(data.level)) || 2),
			);
			return `<h${level}>${html(data.text)}</h${level}>`;
		}
		case 'paragraph':
			return `<p>${html(data.text)}</p>`;
		case 'list':
			return renderList(data.items, data.style, listStart(data.meta));
		case 'quote':
			return `<blockquote><p>${html(data.text)}</p>${caption(
				data.caption,
			)}</blockquote>`;
		case 'warning':
			return `<div class="warning"><strong>${html(
				data.title,
			)}</strong><p>${html(data.message)}</p></div>`;
		case 'delimiter':
			return '<hr>';
		case 'code':
			return `<pre><code>${escapeHtml(data.code)}</code></pre>`;
		case 'codebox':
			return `<pre><code>${escapeHtml(codeboxText(data.code))}</code></pre>`;
		case 'table':
			return renderTable(data);
		case 'raw':
			return html(data.html);
		case 'embed': {
			const url = [data.source, data.embed].find(
				(value) => typeof value === 'string' && /^https?:\/\//i.test(value),
			);
			if (!url) return '';
			const label = escapeHtml(data.service || url);
			return `<figure><a href="${escapeHtml(url)}">${label}</a>${caption(
				data.caption,
			)}</figure>`;
		}
		case 'image': {
			const url = data.file?.url ?? data.url;
			if (typeof url !== 'string' || !url) return '';
			return `<figure><img src="${escapeHtml(url)}" alt="${escapeHtml(
				data.caption?.replace?.(/<[^>]*>/g, ''),
			)}">${caption(data.caption)}</figure>`;
		}
		default:
			return '';
	}
};

const ALLOWED_CLASSES = new Set(['checklist', 'warning']);

// Stricter than comments: a description is shown inside the app chrome, so raw
// blocks must not restyle the page (style, classes) or fake UI (forms).
const sanitize = (body: string): string => {
	const root = DOMPurify.sanitize(body, {
		FORBID_TAGS: ['style', 'form', 'button', 'textarea', 'select'],
		FORBID_ATTR: ['style'],
		RETURN_DOM: true,
	}) as unknown as HTMLElement;
	root.querySelectorAll('input').forEach((input) => {
		if (input.getAttribute('type') === 'checkbox') {
			input.setAttribute('disabled', '');
		} else {
			input.remove();
		}
	});
	root.querySelectorAll('[class]').forEach((element) => {
		if (!ALLOWED_CLASSES.has(element.getAttribute('class') ?? '')) {
			element.removeAttribute('class');
		}
	});
	return root.innerHTML;
};

export default function editorJsToHtml(data: unknown): string {
	const body = normalizeBlocks(data)
		.blocks.filter(
			(block): block is Block => !!block && typeof block === 'object',
		)
		.map(renderBlock)
		.join('');
	return body ? sanitize(body) : '';
}

export const legacyHtmlToHtml = (value: unknown): string =>
	isLegacyHtml(value)
		? editorJsToHtml({ blocks: [{ type: 'raw', data: { html: value } }] })
		: '';
