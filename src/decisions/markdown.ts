import { markdownToHtml } from '@/utils/markdown';
import sanitizeHtml from '@/utils/sanitizeHtml';
import { isHttpUrl } from './format';
import type { CardMessage } from './types';

export const renderMarkdown = (text: string): string =>
	text ? sanitizeHtml(markdownToHtml(text), { remoteMedia: false }) : '';

export const isAgentMessage = (message: Pick<CardMessage, 'author'>): boolean =>
	message.author !== 'owner';

export const clickedHttpLink = (event: MouseEvent): string | null => {
	const anchor = (event.target as HTMLElement).closest('a');
	if (!anchor) return null;
	event.preventDefault();
	const href = anchor.getAttribute('href') ?? '';
	return isHttpUrl(href) ? href : null;
};
