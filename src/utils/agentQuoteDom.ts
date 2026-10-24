import { findQuoteSpan } from '@/utils/agentQuotes';

export const FLASH_CLASS = 'agent-msg-flash';
export const HIGHLIGHT_NAME = 'agent-quote';
const FLASH_MS = 2000;

export interface SelectedQuote {
	messageId: number;
	text: string;
	rect: DOMRect;
}

const elementOf = (node: Node | null): Element | null =>
	node instanceof Element ? node : (node?.parentElement ?? null);

export const selectedQuote = (scope: Element | null): SelectedQuote | null => {
	const selection = window.getSelection();
	if (!scope || !selection || selection.rangeCount === 0) return null;
	if (selection.isCollapsed) return null;
	const text = selection.toString().trim();
	if (!text) return null;
	const range = selection.getRangeAt(0);
	const body = elementOf(range.commonAncestorContainer)?.closest(
		'[data-message-body]',
	);
	if (!body || !scope.contains(body)) return null;
	const id = Number(
		body.closest('[data-message-id]')?.getAttribute('data-message-id'),
	);
	if (!Number.isFinite(id)) return null;
	return { messageId: id, text, rect: range.getBoundingClientRect() };
};

const rangeFor = (root: Element, text: string): Range | null => {
	const walker = document.createTreeWalker(root, NodeFilter.SHOW_TEXT);
	const nodes: Text[] = [];
	for (let n = walker.nextNode(); n; n = walker.nextNode())
		nodes.push(n as Text);
	const span = findQuoteSpan(nodes.map((n) => n.data).join(''), text);
	if (!span) return null;
	const locate = (offset: number, isEnd: boolean) => {
		let seen = 0;
		for (const node of nodes) {
			const next = seen + node.data.length;
			if (isEnd ? offset <= next : offset < next)
				return { node, offset: offset - seen };
			seen = next;
		}
		return null;
	};
	const start = locate(span.start, false);
	const end = locate(span.end, true);
	if (!start || !end) return null;
	const range = document.createRange();
	range.setStart(start.node, start.offset);
	range.setEnd(end.node, end.offset);
	return range;
};

const supportsHighlights = (): boolean =>
	typeof CSS !== 'undefined' &&
	'highlights' in CSS &&
	typeof Highlight !== 'undefined';

let clearFlash: (() => void) | null = null;

export const flashQuote = (el: HTMLElement, text: string | null): void => {
	clearFlash?.();
	const body = el.querySelector('[data-message-body]');
	const range =
		text && body && supportsHighlights() ? rangeFor(body, text) : null;
	if (range) {
		CSS.highlights.set(HIGHLIGHT_NAME, new Highlight(range));
		clearFlash = () => CSS.highlights.delete(HIGHLIGHT_NAME);
	} else {
		el.classList.add(FLASH_CLASS);
		clearFlash = () => el.classList.remove(FLASH_CLASS);
	}
	const clear = clearFlash;
	setTimeout(() => {
		clear();
		if (clearFlash === clear) clearFlash = null;
	}, FLASH_MS);
};
