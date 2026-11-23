import { reactive } from 'vue';
import { speechSynthesisEngine, type TtsEngine, type TtsHooks } from './engine';
import { piperEngine } from './piperEngine';
import { prepareSpeech } from './prepareText';
import { voicePack } from './voicePack';

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
		const { lang, chunks } = prepareSpeech(bodyMd);
		const piper = lang === 'en' && voicePack.supported && voicePack.installed;
		if (!piper && !engine.isAvailable()) return;
		if (!chunks.length) return;
		const token = ++run;
		state.speakingId = messageId;
		const hooks: TtsHooks = {
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
		};
		if (!piper) {
			engine.speak(chunks, lang, hooks);
			return;
		}
		piperEngine.speak(chunks, 'en', {
			onEnd: hooks.onEnd,
			onError(reason) {
				if (reason === 'piper-fallback' && token === run) {
					engine.speak(chunks, lang, hooks);
					return;
				}
				hooks.onError(reason);
			},
		});
	},
	stop() {
		run++;
		piperEngine.stop();
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
