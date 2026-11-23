import { invoke } from '@tauri-apps/api/core';
import type { TtsEngine, TtsHooks } from './engine';
import { normalizeEnglish } from './normalizeEn';
import { voicePack } from './voicePack';

interface Check {
	ok: boolean;
	oov: string[];
}

let audio: AudioContext | null = null;
let run = Date.now();
let active = false;
let sources: AudioBufferSourceNode[] = [];

const context = (): AudioContext => {
	if (!audio || audio.state === 'closed') audio = new AudioContext();
	return audio;
};

const stopBackend = async (through: number): Promise<void> => {
	try {
		await invoke('tts_stop', { run: through });
	} catch {
		return;
	}
};

const halt = (): void => {
	const through = run++;
	const playing = sources;
	sources = [];
	playing.forEach((s) => s.stop());
	if (active) {
		active = false;
		void stopBackend(through);
	}
};

export const piperEngine: TtsEngine = {
	id: 'piper',
	isAvailable: () => typeof AudioContext !== 'undefined',
	speak(chunks, lang, hooks: TtsHooks) {
		halt();
		if (lang !== 'en') {
			hooks.onError('piper-fallback');
			return;
		}
		if (!chunks.length) {
			hooks.onEnd();
			return;
		}
		const gen = ++run;
		const current = () => gen === run;

		let ctx: AudioContext;
		let ready: Promise<unknown>;
		try {
			ctx = context();
			ready = ctx.state === 'suspended' ? ctx.resume() : Promise.resolve();
		} catch {
			hooks.onError('piper-fallback');
			return;
		}

		active = true;
		let started = false;
		let finished = false;
		let pending = 0;
		let nextAt = 0;

		const endIfDone = () => {
			if (!finished || pending > 0 || !current()) return;
			active = false;
			hooks.onEnd();
		};
		const fail = () => {
			if (!current()) return;
			active = false;
			hooks.onError(started ? 'piper-error' : 'piper-fallback');
		};
		const schedule = (samples: Float32Array) => {
			const buffer = ctx.createBuffer(1, samples.length, voicePack.sampleRate);
			buffer.getChannelData(0).set(samples);
			const source = ctx.createBufferSource();
			source.buffer = buffer;
			source.connect(ctx.destination);
			const when = Math.max(nextAt, ctx.currentTime);
			source.onended = () => {
				pending--;
				endIfDone();
			};
			source.start(when);
			nextAt = when + buffer.duration;
			pending++;
			started = true;
			sources.push(source);
		};

		void (async () => {
			try {
				await ready;
				const texts = chunks.map(normalizeEnglish);
				const check = await invoke<Check>('tts_check', { chunks: texts });
				if (!current()) return;
				if (!check.ok) {
					fail();
					return;
				}
				for (const text of texts) {
					const bytes = await invoke<ArrayBuffer>('tts_synthesize', {
						text,
						run: gen,
					});
					if (!current()) return;
					if (bytes.byteLength === 0) continue;
					schedule(new Float32Array(bytes));
				}
				finished = true;
				endIfDone();
			} catch {
				fail();
			}
		})();
	},
	stop: halt,
};
