import type { Answer, CardDelivery, CardView, LineComment } from './types';

export interface AnswerDraft {
	choice?: string | null;
	note?: string;
	freeText?: string;
	lineComments?: LineComment[];
	deferred?: boolean;
	dismissed?: boolean;
}

const clean = (value: string | undefined): string | undefined => {
	const trimmed = (value ?? '').trim();
	return trimmed === '' ? undefined : trimmed;
};

export const buildAnswer = (draft: AnswerDraft): Answer => {
	const answer: Answer = {
		line_comments: (draft.lineComments ?? [])
			.map((comment) => ({ ...comment, text: comment.text.trim() }))
			.filter((comment) => comment.text !== ''),
		deferred: Boolean(draft.deferred),
		dismissed: Boolean(draft.dismissed),
	};
	if (draft.choice) answer.choice = draft.choice;
	const note = clean(draft.note);
	if (note && draft.choice) answer.note = note;
	const free = clean(draft.freeText);
	if (free) answer.free_text = free;
	return answer;
};

export const isEmptyAnswer = (answer: Answer): boolean =>
	!answer.choice &&
	!answer.free_text &&
	!answer.deferred &&
	!answer.dismissed &&
	answer.line_comments.length === 0;

export type CardMode = 'answer' | 'dismiss-only' | 'final';

export const isFinal = (card: CardView): boolean =>
	card.state === 'answered' ||
	card.state === 'closed' ||
	card.state === 'dismissed';

export const cardMode = (card: CardView): CardMode => {
	if (isFinal(card)) return 'final';
	if (
		card.reply_via === 'hook' &&
		!card.source.runner_session &&
		card.push !== true
	) {
		return 'dismiss-only';
	}
	return 'answer';
};

export const answerSummary = (card: CardView, answer: Answer): string => {
	if (answer.dismissed) return 'dismissed';
	if (answer.deferred) return 'deferred';
	const option = card.options.find((o) => o.id === answer.choice);
	const text = option?.label ?? answer.free_text ?? 'answered';
	return answer.note && option ? `${text} + note` : text;
};

export const answerToast = (card: CardView, answer: Answer): string => {
	const title =
		card.title.length > 48 ? `${card.title.slice(0, 47)}…` : card.title;
	return `${title}: ${
		card.session_gone
			? 'saved, but the session has ended'
			: answerSummary(card, answer)
	}`;
};

export const deliveryLabel = (
	delivery: CardDelivery | null | undefined,
): string | null => {
	if (!delivery) return null;
	switch (delivery.state) {
		case 'sent':
			return 'Sent to the session (unconfirmed)';
		case 'delivered':
			return delivery.via === 'runner' ? 'Delivered via runner' : 'Delivered';
		case 'queued':
			return delivery.detail ?? "Waiting for the agent's prompt";
		case 'undelivered':
			return `Not delivered: ${delivery.detail ?? 'session closed'}`;
		default:
			return null;
	}
};

export const UNPICKED_LABEL = 'Not picked up by any agent';

export const unpickedLabel = (card: CardView): string | null =>
	card.unpicked ? UNPICKED_LABEL : null;
