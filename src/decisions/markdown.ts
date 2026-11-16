import { t } from '@/i18n';
import { markdownToHtml } from '@/utils/markdown';
import sanitizeHtml from '@/utils/sanitizeHtml';
import { isHttpUrl } from './format';
import type { CardMessage } from './types';

export const CODE_PREVIEW_LINES = 40;

const CODE_BLOCK = /<pre><code([^>]*)>([\s\S]*?)<\/code><\/pre>/g;

export const collapseLongCode = (html: string): string =>
	html.replace(CODE_BLOCK, (block, attrs: string, body: string) => {
		const lines = body.replace(/\n$/, '').split('\n');
		if (lines.length <= CODE_PREVIEW_LINES) return block;
		const preview = lines.slice(0, CODE_PREVIEW_LINES).join('\n');
		const label = t('decisions.diff.showAllLines', { lines: lines.length });
		return (
			`<div class="bigcode"><pre><code${attrs}>${preview}\n</code></pre>` +
			`<details><summary>${label}</summary>${block}</details></div>`
		);
	});

export const renderMarkdown = (text: string): string =>
	text
		? collapseLongCode(
				sanitizeHtml(markdownToHtml(text, { escapeHtml: true }), {
					remoteMedia: false,
				}),
			)
		: '';

export const isAgentMessage = (message: Pick<CardMessage, 'author'>): boolean =>
	message.author !== 'owner';

export const clickedHttpLink = (event: MouseEvent): string | null => {
	const anchor = (event.target as HTMLElement).closest('a');
	if (!anchor) return null;
	event.preventDefault();
	const href = anchor.getAttribute('href') ?? '';
	return isHttpUrl(href) ? href : null;
};
