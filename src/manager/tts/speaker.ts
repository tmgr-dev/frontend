import { reactive } from 'vue';
import { speechSynthesisEngine, type TtsEngine, type TtsHooks } from './engine';
import { externalEngine } from './externalEngine';
import { externalVoice } from './externalVoice';
import { piperEngine } from './piperEngine';
import { prepareSpeech, type SpeechLang } from './prepareText';
import { voicePacks } from './voicePack';

const CANCELLED = ['canceled', 'interrupted'];

const piperReady = (lang: SpeechLang): boolean =>
	voicePacks[lang].supported && voicePacks[lang].installed;

const anyPackReady = (): boolean => piperReady('en') || piperReady('ru');

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
		return externalVoice.usable || anyPackReady() || engine.isAvailable();
	},
	toggle(messageId, bodyMd) {
		if (state.speakingId === messageId) {
			speaker.stop();
			return;
		}
		speaker.stop();
		const { lang, chunks } = prepareSpeech(bodyMd);
		const steps: Array<() => void> = [];
		let token = 0;
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
		const advance = (reason: string) => {
			const next = steps.shift();
			if (!next) {
				hooks.onError(reason);
				return;
			}
			next();
		};
		const chained = (fallback: string, afterFail?: () => void): TtsHooks => ({
			onEnd: hooks.onEnd,
			onError(reason) {
				if (token !== run) return;
				if (reason === fallback || reason.endsWith('-error')) afterFail?.();
				if (reason === fallback) {
					advance(reason);
					return;
				}
				hooks.onError(reason);
			},
		});
		if (externalVoice.usable && externalEngine.isAvailable()) {
			steps.push(() =>
				externalEngine.speak(
					chunks,
					lang,
					chained('external-fallback', () => externalVoice.markUnhealthy()),
				),
			);
		}
		if (piperReady(lang) && piperEngine.isAvailable()) {
			steps.push(() =>
				piperEngine.speak(chunks, lang, chained('piper-fallback')),
			);
		}
		if (engine.isAvailable()) {
			steps.push(() => engine.speak(chunks, lang, hooks));
		}
		if (!steps.length || !chunks.length) return;
		token = ++run;
		state.speakingId = messageId;
		advance('no-voice');
	},
	stop() {
		run++;
		externalEngine.stop();
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
