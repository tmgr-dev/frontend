import { reactive } from 'vue';
import { buildAnswer, cardMode } from '../decisions/answer';
import type { Answer, CardView } from '../decisions/types';

export interface DecisionCardApi {
	get: (id: string) => Promise<CardView>;
	answer: (id: string, answer: Answer) => Promise<void>;
}

export type AnswerState = 'answer' | 'final' | 'action';

export const answerState = (card: CardView): AnswerState => {
	if (cardMode(card) === 'final') return 'final';
	if (card.action || cardMode(card) !== 'answer') return 'action';
	return 'answer';
};

const stamped = (card: CardView, answer: Answer): Answer => ({
	...answer,
	seen_ms: card.updated_ms ?? card.created_ms,
});

export const choiceAnswer = (card: CardView, choice: string): Answer =>
	stamped(card, buildAnswer({ choice }));

export const freeTextAnswer = (card: CardView, text: string): Answer | null => {
	const answer = buildAnswer({ freeText: text });
	return answer.free_text ? stamped(card, answer) : null;
};

export interface DecisionCardState {
	card: CardView | null;
	error: string;
	busy: boolean;
}

export const createDecisionCardLoader = (api: DecisionCardApi, id: string) => {
	const state = reactive<DecisionCardState>({
		card: null,
		error: '',
		busy: false,
	});
	const load = async () => {
		try {
			state.card = await api.get(id);
			state.error = '';
		} catch (e) {
			state.error = String(e);
		}
	};
	const submit = async (answer: Answer) => {
		state.busy = true;
		try {
			await api.answer(id, answer);
		} catch (e) {
			state.error = String(e);
			state.busy = false;
			return;
		}
		state.busy = false;
		await load();
	};
	return { state, load, submit };
};
