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

const voicesReady = (s: SpeechSynthesis): Promise<SpeechSynthesisVoice[]> => {
	const ready = s.getVoices();
	if (ready.length) return Promise.resolve(ready);
	return new Promise((resolve) => {
		const done = () => {
			clearTimeout(timer);
			s.removeEventListener('voiceschanged', done);
			resolve(s.getVoices());
		};
		const timer = setTimeout(done, VOICE_WAIT_MS);
		s.addEventListener('voiceschanged', done);
	});
};

let generation = 0;

export const speechSynthesisEngine: TtsEngine = {
	id: 'speechSynthesis',
	isAvailable: () => synth() !== undefined,
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
		void voicesReady(s).then((voices) => {
			if (gen !== generation) return;
			const voice = voices.find((v) => v.lang.toLowerCase().startsWith(lang));
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
					if (CANCELLED.includes(e.error)) return;
					settle(() => hooks.onError(e.error || 'error'));
				};
				s.speak(u);
			});
		});
	},
	stop() {
		generation++;
		synth()?.cancel();
	},
};
