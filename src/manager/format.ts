import { renderMarkdown } from '@/decisions/markdown';
import type { ChatMessage } from './types';

export type StatusTone = 'working' | 'waiting' | 'failed' | 'idle' | 'done';
export type ShownKind = 'text' | 'decision' | 'system';

export type TimelineItem =
	| { type: 'day'; key: string; ts: number }
	| { type: 'message'; message: ChatMessage; grouped: boolean };

const MONTHS = [
	'Jan',
	'Feb',
	'Mar',
	'Apr',
	'May',
	'Jun',
	'Jul',
	'Aug',
	'Sep',
	'Oct',
	'Nov',
	'Dec',
];

const TONES: readonly StatusTone[] = [
	'working',
	'waiting',
	'failed',
	'idle',
	'done',
];

export const formatCtx = (tokens: number): string =>
	tokens >= 1000 ? `${Math.round(tokens / 1000)}k` : String(tokens);

const pad = (n: number) => String(n).padStart(2, '0');

export const dayKey = (ts: number): string => {
	const d = new Date(ts);
	return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}`;
};

export const formatTime = (ts: number, now: number): string => {
	const d = new Date(ts);
	if (dayKey(ts) === dayKey(now)) {
		return `${pad(d.getHours())}:${pad(d.getMinutes())}`;
	}
	const base = `${MONTHS[d.getMonth()]} ${d.getDate()}`;
	return d.getFullYear() === new Date(now).getFullYear()
		? base
		: `${base}, ${d.getFullYear()}`;
};

export const statusTone = (state?: string): StatusTone =>
	TONES.find((tone) => tone === state) ?? 'idle';

export const shownKind = (message: Pick<ChatMessage, 'kind'>): ShownKind =>
	message.kind === 'text' || message.kind === 'decision'
		? message.kind
		: 'system';

export const groupMessages = (messages: ChatMessage[]): TimelineItem[] => {
	const items: TimelineItem[] = [];
	let prev: ChatMessage | null = null;
	for (const message of messages) {
		const key = dayKey(message.ts_ms);
		const newDay = !prev || dayKey(prev.ts_ms) !== key;
		if (newDay) items.push({ type: 'day', key, ts: message.ts_ms });
		const grouped =
			!newDay &&
			prev !== null &&
			shownKind(prev) === 'text' &&
			shownKind(message) === 'text' &&
			prev.author.kind === message.author.kind &&
			prev.author.id === message.author.id;
		items.push({ type: 'message', message, grouped });
		prev = message;
	}
	return items;
};

export const quotePreview = (bodyMd: string, max = 120): string => {
	const text = bodyMd
		.replace(/```/g, ' ')
		.replace(/!\[([^\]]*)\]\([^)]*\)/g, '$1')
		.replace(/\[([^\]]*)\]\([^)]*\)/g, '$1')
		.replace(/^\s{0,3}(#{1,6}|>|[-*+]|\d+\.)\s+/gm, '')
		.replace(/[`*_~]/g, '')
		.replace(/\s+/g, ' ')
		.trim();
	return text.length > max ? `${text.slice(0, max - 1)}…` : text;
};

export const renderBody = (bodyMd: string): string => renderMarkdown(bodyMd);
