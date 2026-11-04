import type { CardMessage, CardView, DiffLine, DiffView } from './types';

export interface LineAnchor {
	fileIndex: number;
	hunkIndex: number;
	lineIndex: number;
}

export const lineNumber = (line: DiffLine): number | null =>
	line.new ?? line.old ?? null;

export const threadKey = (file: string, line: number): string =>
	`${file}:${line}`;

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
				const key = threadKey(
					label === null ? file.path : qualifyFile(label, file.path, 2),
					number,
				);
				if (!anchors.has(key)) {
					anchors.set(key, { fileIndex, hunkIndex, lineIndex });
				}
			});
		});
	});
	return anchors;
};

export const messageThreadKey = (message: CardMessage): string | null =>
	message.file !== undefined && message.line !== undefined
		? threadKey(message.file, message.line)
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
	if (ctx.sessionGone) return 'The session ended, so no agent will see this.';
	if (!ctx.listening) {
		return "No agent is listening right now; it'll see this when it comes back.";
	}
	if (ctx.now - last.created_ms < AGENT_WAIT_MS) return 'waiting for the agent…';
	return "The agent hasn't replied yet; it'll see this when it next checks.";
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
