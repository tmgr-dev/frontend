import type { AgentCompaction, AgentMessage } from '@/types/agent';

export const mergeCompaction = (
	compactions: AgentCompaction[],
	next: AgentCompaction,
): AgentCompaction[] => {
	if (compactions.some((c) => c.id === next.id)) return compactions;
	return [...compactions, next].sort((a, b) => a.id - b.id);
};

export const compactionsByAnchor = (
	compactions: AgentCompaction[],
): Map<number, AgentCompaction[]> => {
	const anchors = new Map<number, AgentCompaction[]>();
	for (const c of compactions) {
		anchors.set(c.up_to_message_id, [
			...(anchors.get(c.up_to_message_id) ?? []),
			c,
		]);
	}
	return anchors;
};

export const canCompactConversation = (
	messages: AgentMessage[],
	blocked: boolean,
): boolean =>
	!blocked &&
	messages.filter((m) => m.status === 'done').length >= 2 &&
	!messages.some((m) => m.status !== 'done' && m.status !== 'failed');

export const compactErrorMessage = (
	status: number | undefined,
	serverMessage?: string,
): string => {
	switch (status) {
		case 409:
			return serverMessage?.trim() || 'Nothing to compact right now.';
		case 429:
			return 'Compacting is rate limited. Try again later.';
		default:
			return 'Could not compact the history. Please try again.';
	}
};
