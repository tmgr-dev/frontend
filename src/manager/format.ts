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
	message.kind === 'text'
		? 'text'
		: message.kind === 'decision'
			? 'decision'
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
			prev.author.id === message.author.id &&
			!!prev.user_turn === !!message.user_turn;
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

export interface CopyLabels {
	copy: string;
	copied: string;
}

const COPIED_MS = 1500;
const copyTimers = new WeakMap<HTMLElement, number>();

const copyButton = (labels: CopyLabels): HTMLButtonElement => {
	const btn = document.createElement('button');
	btn.type = 'button';
	btn.className = 'blk-copy';
	btn.textContent = labels.copy;
	btn.setAttribute('aria-label', labels.copy);
	btn.dataset.copyLabel = labels.copy;
	btn.dataset.copiedLabel = labels.copied;
	return btn;
};

export const addCopyButtons = (html: string, labels: CopyLabels): string => {
	const tpl = document.createElement('template');
	tpl.innerHTML = html;
	const targets = Array.from(
		tpl.content.querySelectorAll('pre, blockquote, .bigcode'),
	).filter((el) =>
		el.tagName === 'PRE'
			? !el.closest('.bigcode')
			: el.tagName !== 'BLOCKQUOTE' || !el.parentElement?.closest('blockquote'),
	);
	for (const el of targets) {
		const blk = document.createElement('div');
		blk.className = 'blk';
		el.before(blk);
		blk.append(copyButton(labels), el);
	}
	return tpl.innerHTML;
};

const codeText = (code: Element | null): string =>
	(code?.textContent ?? '').replace(/\n$/, '');

const quoteText = (quote: Element): string => {
	const clone = quote.cloneNode(true) as Element;
	clone
		.querySelectorAll('.blk-copy, .bigcode > pre')
		.forEach((el) => el.remove());
	clone.querySelectorAll('br').forEach((br) => br.replaceWith('\n'));
	const leaves = Array.from(clone.querySelectorAll('p, li, pre')).filter(
		(el) => !el.querySelector('p, li, pre'),
	);
	if (leaves.length === 0) return (clone.textContent ?? '').trim();
	return leaves
		.map((el) =>
			el.tagName === 'PRE'
				? codeText(el.querySelector('code'))
				: (el.textContent ?? '').trim(),
		)
		.join('\n');
};

export const copyBlockText = (block: Element | null): string => {
	const target = block?.lastElementChild;
	if (!target) return '';
	if (target.tagName === 'BLOCKQUOTE') return quoteText(target);
	if (target.classList.contains('bigcode'))
		return codeText(target.querySelector('details pre code'));
	return codeText(target.querySelector('code'));
};

const execCopy = (text: string): boolean => {
	const area = document.createElement('textarea');
	area.value = text;
	area.setAttribute('readonly', '');
	area.style.position = 'fixed';
	area.style.opacity = '0';
	document.body.appendChild(area);
	area.select();
	try {
		return document.execCommand('copy');
	} catch {
		return false;
	} finally {
		area.remove();
	}
};

export const copyText = async (text: string): Promise<boolean> => {
	try {
		await navigator.clipboard.writeText(text);
		return true;
	} catch {
		return execCopy(text);
	}
};

const setButtonLabel = (btn: HTMLElement, label: string): void => {
	btn.textContent = label;
	btn.setAttribute('aria-label', label);
};

export const flashCopied = (btn: HTMLElement): void => {
	window.clearTimeout(copyTimers.get(btn));
	setButtonLabel(btn, btn.dataset.copiedLabel ?? '');
	copyTimers.set(
		btn,
		window.setTimeout(
			() => setButtonLabel(btn, btn.dataset.copyLabel ?? ''),
			COPIED_MS,
		),
	);
};
