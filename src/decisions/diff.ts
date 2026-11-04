import type { CardMessage, DiffLine, DiffView } from './types';

export interface LineAnchor {
	fileIndex: number;
	hunkIndex: number;
	lineIndex: number;
}

export const lineNumber = (line: DiffLine): number | null =>
	line.new ?? line.old ?? null;

export const threadKey = (file: string, line: number): string =>
	`${file}:${line}`;

export const buildAnchors = (diff: DiffView): Map<string, LineAnchor> => {
	const anchors = new Map<string, LineAnchor>();
	diff.files.forEach((file, fileIndex) => {
		file.hunks.forEach((hunk, hunkIndex) => {
			hunk.lines.forEach((line, lineIndex) => {
				const number = lineNumber(line);
				if (number === null) return;
				const key = threadKey(file.path, number);
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
