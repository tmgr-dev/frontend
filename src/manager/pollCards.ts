import type { ChatMessage, PollWire } from './types';

export interface PollClose {
	closed: string;
	picked?: string;
	why?: string;
}

export interface PollGroup {
	card: ChatMessage & { poll: PollWire };
	closed?: PollClose;
}

export const hasPollQuestions = (m: ChatMessage): boolean =>
	!!m.poll?.questions?.length;

export const pollGroups = (
	messages: ChatMessage[],
): Record<string, PollGroup> => {
	const groups: Record<string, PollGroup> = {};
	const open: Record<string, string> = {};
	const seen = new Set<string>();
	const early: Record<string, PollClose> = {};
	for (const m of messages) {
		const poll = m.poll;
		if (!poll?.card_id) continue;
		const id = poll.card_id;
		if (poll.questions?.length) {
			const card = { ...m, poll };
			const first = open[id];
			if (first) {
				groups[first].card = card;
				continue;
			}
			open[id] = m.id;
			seen.add(id);
			groups[m.id] = { card };
			if (early[id]) {
				groups[m.id].closed = early[id];
				delete early[id];
				delete open[id];
			}
		} else if (poll.closed) {
			const closed = {
				closed: poll.closed,
				picked: poll.picked,
				why: poll.why_not,
			};
			if (open[id]) {
				groups[open[id]].closed = closed;
				delete open[id];
			} else if (!seen.has(id) && !early[id]) {
				early[id] = closed;
			}
		}
	}
	return groups;
};

export const pollHidden = (
	m: ChatMessage,
	groups: Record<string, PollGroup>,
): boolean => {
	if (!m.poll) return false;
	return hasPollQuestions(m) ? !groups[m.id] : true;
};
