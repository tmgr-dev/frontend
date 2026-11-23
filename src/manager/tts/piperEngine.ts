import { invoke } from '@tauri-apps/api/core';
import { openPlayback, type Playback } from './audioPlayback';
import type { TtsEngine, TtsHooks } from './engine';
import { normalizeEnglish } from './normalizeEn';
import { normalizeRussian } from './normalizeRu';
import { voicePacks } from './voicePack';

interface Check {
	ok: boolean;
	oov: string[];
}

let run = Date.now();
let active = false;
let playback: Playback | null = null;

const stopBackend = async (through: number): Promise<void> => {
	try {
		await invoke('tts_stop', { run: through });
	} catch {
		return;
	}
};

const halt = (): void => {
	const through = run++;
	const playing = playback;
	playback = null;
	playing?.stop();
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
		if (!chunks.length) {
			hooks.onEnd();
			return;
		}
		const gen = ++run;
		const current = () => gen === run;

		let mine: Playback;
		try {
			mine = openPlayback(() => {
				if (!current()) return;
				active = false;
				hooks.onEnd();
			});
		} catch {
			hooks.onError('piper-fallback');
			return;
		}
		playback = mine;
		active = true;

		const fail = () => {
			if (!current()) return;
			active = false;
			const reason = mine.started ? 'piper-error' : 'piper-fallback';
			mine.stop();
			hooks.onError(reason);
		};

		void (async () => {
			try {
				await mine.ready;
				const normalize = lang === 'ru' ? normalizeRussian : normalizeEnglish;
				const texts = chunks.map(normalize);
				const check = await invoke<Check>('tts_check', { chunks: texts, lang });
				if (!current()) return;
				if (!check.ok) {
					fail();
					return;
				}
				for (const text of texts) {
					const bytes = await invoke<ArrayBuffer>('tts_synthesize', {
						text,
						run: gen,
						lang,
					});
					if (!current()) return;
					if (bytes.byteLength === 0) continue;
					mine.schedule(new Float32Array(bytes), voicePacks[lang].sampleRate);
				}
				mine.finish();
			} catch {
				fail();
			}
		})();
	},
	stop: halt,
};
