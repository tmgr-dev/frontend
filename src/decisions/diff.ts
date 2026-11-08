import { t } from '@/i18n';
import type {
	CardMessage,
	CardView,
	DiffGroup,
	DiffLine,
	DiffView,
	ThreadSide,
} from './types';

export interface LineAnchor {
	fileIndex: number;
	hunkIndex: number;
	lineIndex: number;
	key: string;
}

export const lineNumber = (line: DiffLine): number | null =>
	line.new ?? line.old ?? null;

export const sideOf = (line: DiffLine): ThreadSide =>
	line.kind === 'del' ? 'old' : 'new';

export const threadKey = (
	file: string,
	line: number,
	side?: ThreadSide,
): string => (side ? `${file}:${line}:${side}` : `${file}:${line}`);

export const qualifyFile = (
	label: string,
	path: string,
	groupCount: number,
): string => (groupCount > 1 ? `${label}:${path}` : path);

export const hasChanges = (card: CardView): boolean =>
	Boolean(card.diff) || card.diffs.length > 0 || card.patches.length > 0;

export const buildAnchors = (
	diff: DiffView,
	label: string | null = null,
): Map<string, LineAnchor> => {
	const anchors = new Map<string, LineAnchor>();
	diff.files.forEach((file, fileIndex) => {
		file.hunks.forEach((hunk, hunkIndex) => {
			hunk.lines.forEach((line, lineIndex) => {
				const number = lineNumber(line);
				if (number === null) return;
				const path =
					label === null ? file.path : qualifyFile(label, file.path, 2);
				const sided = threadKey(path, number, sideOf(line));
				const anchor = { fileIndex, hunkIndex, lineIndex, key: sided };
				if (!anchors.has(sided)) anchors.set(sided, anchor);
				const plain = threadKey(path, number);
				if (!anchors.has(plain)) anchors.set(plain, anchor);
			});
		});
	});
	return anchors;
};

export const messageThreadKey = (message: CardMessage): string | null =>
	message.file !== undefined && message.line !== undefined
		? threadKey(message.file, message.line, message.side)
		: null;

export const splitMessages = (
	messages: CardMessage[],
): { threads: Map<string, CardMessage[]>; chat: CardMessage[] } => {
	const threads = new Map<string, CardMessage[]>();
	const chat: CardMessage[] = [];
	const ordered = [...messages].sort((a, b) => a.seq - b.seq);
	for (const message of ordered) {
		const key = messageThreadKey(message);
		if (key === null) {
			chat.push(message);
			continue;
		}
		const list = threads.get(key);
		if (list) list.push(message);
		else threads.set(key, [message]);
	}
	return { threads, chat };
};

export const remapLegacyThreads = (
	threads: Map<string, CardMessage[]>,
	anchors: Map<string, LineAnchor>[],
): Map<string, CardMessage[]> => {
	const out = new Map<string, CardMessage[]>();
	for (const [key, list] of threads) {
		const plain = key.replace(/:(old|new)$/, '');
		const found = (k: string) =>
			anchors.map((a) => a.get(k)).find((a) => a !== undefined);
		const target = (found(key) ?? found(plain))?.key ?? key;
		const merged = [...(out.get(target) ?? []), ...list];
		out.set(
			target,
			merged.sort((a, b) => a.seq - b.seq),
		);
	}
	return out;
};

export const awaitsOwner = (thread: CardMessage[]): boolean =>
	thread.length > 0 && thread[thread.length - 1].author === 'agent';

export const fixMessageText = (extra: string): string => {
	const text = extra.trim();
	return text === '' ? 'Fix this.' : `Fix: ${text}`;
};

export const AGENT_WAIT_MS = 3 * 60 * 1000;

export interface WaitContext {
	listening: boolean;
	sessionGone: boolean;
	canReply: boolean;
	final: boolean;
	now: number;
}

export const agentWaitNotice = (
	messages: CardMessage[],
	ctx: WaitContext,
): string | null => {
	const last = messages[messages.length - 1];
	if (!last || last.author !== 'owner' || ctx.final || !ctx.canReply) {
		return null;
	}
	if (ctx.sessionGone) return t('decisions.wait.sessionGone');
	const delivered = last.delivered === true;
	if (!ctx.listening && !delivered) {
		return t('decisions.wait.notListening');
	}
	if (ctx.now - last.created_ms < AGENT_WAIT_MS)
		return t('decisions.wait.waiting');
	return delivered
		? t('decisions.wait.noReplyDelivered')
		: t('decisions.wait.noReplyPending');
};

export type RequestSeqs = Record<string, number>;

export const beginRequest = (seqs: RequestSeqs, id: string): number => {
	seqs[id] = (seqs[id] ?? 0) + 1;
	return seqs[id];
};

export const isLatestRequest = (
	seqs: RequestSeqs,
	id: string,
	ticket: number,
): boolean => seqs[id] === ticket;

export interface DiffTotals {
	added: number;
	removed: number;
	exact: boolean;
}

export const diffTotals = (groups: DiffGroup[]): DiffTotals => {
	const totals: DiffTotals = { added: 0, removed: 0, exact: true };
	for (const { view } of groups) {
		if (view.truncated || view.error) totals.exact = false;
		for (const file of view.files) {
			totals.added += file.added;
			totals.removed += file.removed;
			if (file.binary) totals.exact = false;
		}
	}
	return totals;
};
