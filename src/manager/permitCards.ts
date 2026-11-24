import type { ChatMessage, DecisionRef, PermitCardWire } from './types';

export const permitKey = (poster: string | undefined, cardId: string) =>
	`${poster ?? ''}|${cardId}`;

export interface PermitGroup {
	decision: DecisionRef & { permit: PermitCardWire };
	closed?: string;
}

export const permitGroups = (
	messages: ChatMessage[],
): Record<string, PermitGroup> => {
	const groups: Record<string, PermitGroup> = {};
	const open: Record<string, string> = {};
	for (const m of messages) {
		const d = m.decision;
		if (!d?.card_id) continue;
		const key = permitKey(d.poster, d.card_id);
		const permit = d.permit;
		if (permit) {
			const decision = { ...d, permit };
			const first = open[key];
			if (first) groups[first].decision = decision;
			else {
				open[key] = m.id;
				groups[m.id] = { decision };
			}
		} else if (d.closed && open[key]) {
			groups[open[key]].closed = d.closed;
			delete open[key];
		}
	}
	return groups;
};

const FORMAT_CONTROLS = /[‎‏‪-‮⁦-⁩؜]/g;

export const stripFormat = (s: string): string =>
	s.replace(FORMAT_CONTROLS, '');
