import type { ChatMessage } from './types';

export const permitKey = (poster: string | undefined, cardId: string) =>
	`${poster ?? ''}|${cardId}`;

export const newestPermits = (
	messages: ChatMessage[],
): Record<string, string> => {
	const newest: Record<string, string> = {};
	for (const m of messages) {
		const d = m.decision;
		if (d?.permit) newest[permitKey(d.poster, d.card_id)] = m.id;
	}
	return newest;
};

export const closedCards = (
	messages: ChatMessage[],
): Record<string, string> => {
	const closed: Record<string, string> = {};
	for (const m of messages) {
		const d = m.decision;
		if (d?.closed && !d.permit)
			closed[permitKey(d.poster, d.card_id)] = d.closed;
	}
	return closed;
};

const FORMAT_CONTROLS = /[‎‏‪-‮⁦-⁩؜]/g;

export const stripFormat = (s: string): string =>
	s.replace(FORMAT_CONTROLS, '');
