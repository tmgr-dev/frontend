import { t } from '@/i18n';
import type {
	Answer,
	AnswerChoice,
	CardDelivery,
	CardView,
	LineComment,
} from './types';

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
	if (answer.dismissed) return t('decisions.answer.dismissed');
	if (answer.deferred) return t('decisions.answer.deferred');
	if (answer.choices?.length) {
		const labels = answer.choices.map((c) => c.label).join(', ');
		return answer.choices.some((c) => c.note)
			? t('decisions.answer.withNote', { text: labels })
			: labels;
	}
	const option = card.options.find((o) => o.id === answer.choice);
	const text =
		option?.label ?? answer.free_text ?? t('decisions.answer.answered');
	return answer.note && option
		? t('decisions.answer.withNote', { text })
		: text;
};

export const answerChoices = (card: CardView): AnswerChoice[] =>
	card.answer?.choices ?? [];

export const answerDetail = (card: CardView): string => {
	const { answer, options } = card;
	if (!answer) return '';
	if (answer.choices?.length) return answer.free_text ?? '';
	const option = options.find((o) => o.id === answer.choice);
	return [option?.label, answer.note, answer.free_text]
		.filter(Boolean)
		.join(' — ');
};

export const answerToast = (card: CardView, answer: Answer): string => {
	const title =
		card.title.length > 48 ? `${card.title.slice(0, 47)}…` : card.title;
	return t('decisions.answer.toast', {
		title,
		summary: card.session_gone
			? t('decisions.answer.sessionGone')
			: answerSummary(card, answer),
	});
};

export const RUNNER_QUEUE_DETAIL = "Waiting for the agent's prompt";

export const deliveryLabel = (
	delivery: CardDelivery | null | undefined,
): string | null => {
	if (!delivery) return null;
	switch (delivery.state) {
		case 'sent':
			return t('decisions.delivery.sent');
		case 'delivered':
			return delivery.via === 'runner'
				? t('decisions.delivery.deliveredRunner')
				: t('decisions.delivery.delivered');
		case 'queued':
			return delivery.detail ?? t('decisions.delivery.queued');
		case 'undelivered':
			return t('decisions.delivery.undelivered', {
				reason: delivery.detail ?? t('decisions.delivery.sessionClosed'),
			});
		default:
			return null;
	}
};

export const isTypedQueue = (
	delivery: CardDelivery | null | undefined,
): boolean =>
	delivery?.state === 'queued' &&
	delivery.via === 'runner' &&
	delivery.detail === RUNNER_QUEUE_DETAIL;

export const isTypedDelivered = (
	delivery: CardDelivery | null | undefined,
): boolean => delivery?.state === 'delivered' && delivery.via === 'runner';

export const unpickedLabel = (card: CardView): string | null =>
	card.unpicked ? t('decisions.unpicked') : null;

export const listenLabel = (card: CardView): string | null => {
	if (
		(card.state !== 'open' && card.state !== 'deferred') ||
		(card.reply_via !== 'mcp' && card.reply_via !== 'cli')
	) {
		return null;
	}
	return card.listening ? t('decisions.listen.on') : t('decisions.listen.off');
};
