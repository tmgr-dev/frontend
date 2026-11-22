export interface TtsHooks {
	onEnd(): void;
	onError(reason: string): void;
}

export interface TtsEngine {
	readonly id: string;
	isAvailable(): boolean;
	speak(chunks: string[], lang: 'ru' | 'en', hooks: TtsHooks): void;
	stop(): void;
}

const VOICE_WAIT_MS = 1500;
const CANCELLED = ['canceled', 'interrupted'];

const synth = (): SpeechSynthesis | undefined =>
	typeof globalThis.speechSynthesis === 'undefined' ||
	typeof globalThis.SpeechSynthesisUtterance === 'undefined'
		? undefined
		: globalThis.speechSynthesis;

const pickVoice = (
	voices: SpeechSynthesisVoice[],
	lang: 'ru' | 'en',
): SpeechSynthesisVoice | undefined =>
	voices.find(
		(v) => v.localService !== false && v.lang.toLowerCase().startsWith(lang),
	);

let generation = 0;

export const speechSynthesisEngine: TtsEngine = {
	id: 'speechSynthesis',
	isAvailable() {
		const s = synth();
		if (!s) return false;
		s.getVoices();
		return true;
	},
	speak(chunks, lang, hooks) {
		if (!chunks.length) {
			hooks.onEnd();
			return;
		}
		const s = synth();
		if (!s) {
			hooks.onError('unavailable');
			return;
		}
		const gen = ++generation;
		let settled = false;
		const settle = (fn: () => void) => {
			if (settled || gen !== generation) return;
			settled = true;
			fn();
		};
		const start = (voices: SpeechSynthesisVoice[]) => {
			const voice = pickVoice(voices, lang);
			if (!voice) {
				settle(() => hooks.onError('no-voice'));
				return;
			}
			chunks.forEach((text, i) => {
				const u = new SpeechSynthesisUtterance(text);
				u.voice = voice;
				u.lang = voice.lang;
				if (i === chunks.length - 1) u.onend = () => settle(hooks.onEnd);
				u.onerror = (e) => {
					if (CANCELLED.includes(e.error)) {
						settle(hooks.onEnd);
						return;
					}
					settle(() => {
						s.cancel();
						hooks.onError(e.error || 'error');
					});
				};
				s.speak(u);
			});
		};
		const ready = s.getVoices();
		if (ready.length) {
			start(ready);
			return;
		}
		const done = () => {
			clearTimeout(timer);
			s.removeEventListener('voiceschanged', done);
			if (gen === generation) start(s.getVoices());
		};
		const timer = setTimeout(done, VOICE_WAIT_MS);
		s.addEventListener('voiceschanged', done);
	},
	stop() {
		generation++;
		synth()?.cancel();
	},
};
