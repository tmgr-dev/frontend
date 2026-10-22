import type { AgentMessage, AgentQuote, QuoteDraft } from '@/types/agent';
import { formatMessageTime } from '@/utils/agentMessageMeta';

export const MAX_QUOTES = 5;
export const MAX_QUOTE_TEXT = 4000;
export const QUOTE_LIMIT_MESSAGE = `You can quote up to ${MAX_QUOTES} messages.`;
export const QUOTE_UNAVAILABLE = 'Original message unavailable';

export const addQuote = (
	quotes: QuoteDraft[],
	draft: QuoteDraft,
): QuoteDraft[] => {
	const text = draft.text === null ? null : draft.text.trim();
	if (text === '') return quotes;
	if (
		quotes.some(
			(q) =>
				q.message_id === draft.message_id &&
				(q.text === null || text === null || q.text === text),
		)
	)
		return quotes;
	if (quotes.length >= MAX_QUOTES) return quotes;
	return [
		...quotes,
		{
			message_id: draft.message_id,
			text: text === null ? null : text.slice(0, MAX_QUOTE_TEXT),
		},
	];
};

export const isQuoteListFull = (quotes: QuoteDraft[]): boolean =>
	quotes.length >= MAX_QUOTES;

export const removeQuote = (
	quotes: QuoteDraft[],
	index: number,
): QuoteDraft[] => quotes.filter((_, i) => i !== index);

export const quoteAuthor = (role: 'user' | 'assistant'): string =>
	role === 'user' ? 'You' : 'AI';

export const truncateQuote = (text: string, max = 80): string => {
	const flat = text.replace(/\s+/g, ' ').trim();
	return flat.length > max ? `${flat.slice(0, max - 1).trimEnd()}…` : flat;
};

export interface QuoteChip {
	author: string;
	time: string;
	text: string;
}

export const draftChip = (
	draft: QuoteDraft,
	messages: Pick<AgentMessage, 'id' | 'role' | 'content' | 'created_at'>[],
): QuoteChip => {
	const message = messages.find((m) => m.id === draft.message_id);
	return {
		author: message ? quoteAuthor(message.role) : 'Message',
		time: message ? formatMessageTime(message.created_at) : '',
		text: truncateQuote(draft.text ?? message?.content ?? ''),
	};
};

export const sentChip = (quote: AgentQuote): QuoteChip => ({
	author: quoteAuthor(quote.role),
	time: formatMessageTime(quote.created_at),
	text: truncateQuote(quote.text),
});

const stripWhitespace = (s: string): { out: string; index: number[] } => {
	let out = '';
	const index: number[] = [];
	for (let i = 0; i < s.length; i++) {
		if (/\s/.test(s[i])) continue;
		out += s[i];
		index.push(i);
	}
	return { out, index };
};

export const findQuoteSpan = (
	haystack: string,
	needle: string,
): { start: number; end: number } | null => {
	const wanted = stripWhitespace(needle).out;
	if (!wanted) return null;
	const flat = stripWhitespace(haystack);
	const at = flat.out.indexOf(wanted);
	if (at === -1) return null;
	return {
		start: flat.index[at],
		end: flat.index[at + wanted.length - 1] + 1,
	};
};
