import { reactive } from 'vue';
import { speechSynthesisEngine, type TtsEngine } from './engine';
import { prepareSpeech } from './prepareText';

const CANCELLED = ['canceled', 'interrupted'];

interface Speaker {
	readonly speakingId: string | null;
	readonly error: string | null;
	readonly errorId: string | null;
	readonly available: boolean;
	toggle(messageId: string, bodyMd: string): void;
	stop(): void;
}

let engine: TtsEngine = speechSynthesisEngine;
let run = 0;

const state = reactive({
	speakingId: null as string | null,
	error: null as string | null,
	errorId: null as string | null,
});

export const speaker: Speaker = {
	get speakingId() {
		return state.speakingId;
	},
	get error() {
		return state.error;
	},
	get errorId() {
		return state.errorId;
	},
	get available() {
		return engine.isAvailable();
	},
	toggle(messageId, bodyMd) {
		if (state.speakingId === messageId) {
			speaker.stop();
			return;
		}
		speaker.stop();
		if (!engine.isAvailable()) return;
		const { lang, chunks } = prepareSpeech(bodyMd);
		if (!chunks.length) return;
		const token = ++run;
		state.speakingId = messageId;
		engine.speak(chunks, lang, {
			onEnd() {
				if (token === run) state.speakingId = null;
			},
			onError(reason) {
				if (token !== run) return;
				state.speakingId = null;
				if (CANCELLED.includes(reason)) return;
				state.error = reason;
				state.errorId = messageId;
			},
		});
	},
	stop() {
		run++;
		engine.stop();
		state.speakingId = null;
		state.error = null;
		state.errorId = null;
	},
};

export const setEngine = (next: TtsEngine): void => {
	speaker.stop();
	engine = next;
};
