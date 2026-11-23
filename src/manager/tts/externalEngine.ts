import { invoke } from '@tauri-apps/api/core';
import { openPlayback, type Playback } from './audioPlayback';
import type { TtsEngine, TtsHooks } from './engine';
import { normalizeEnglish } from './normalizeEn';
import { normalizeRussian } from './normalizeRu';
import { nextRun } from './runCounter';

export interface Decoded {
	sampleRate: number;
	samples: Float32Array;
}

export const decodeExternal = (bytes: ArrayBuffer): Decoded | null => {
	if (bytes.byteLength <= 4) return null;
	const sampleRate = new DataView(bytes).getUint32(0, true);
	return { sampleRate, samples: new Float32Array(bytes, 4) };
};

let latest = 0;
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
	const through = latest;
	latest = nextRun();
	const playing = playback;
	playback = null;
	playing?.stop();
	if (active) {
		active = false;
		void stopBackend(through);
	}
};

export const externalEngine: TtsEngine = {
	id: 'external',
	isAvailable: () => typeof AudioContext !== 'undefined',
	speak(rawChunks, lang, hooks: TtsHooks) {
		halt();
		const normalize = lang === 'ru' ? normalizeRussian : normalizeEnglish;
		const chunks = rawChunks.map(normalize).filter((c) => c.length > 0);
		if (!chunks.length) {
			hooks.onEnd();
			return;
		}
		const gen = nextRun();
		latest = gen;
		const current = () => gen === latest;

		let mine: Playback;
		try {
			mine = openPlayback(() => {
				if (!current()) return;
				active = false;
				hooks.onEnd();
			});
		} catch {
			hooks.onError('external-fallback');
			return;
		}
		playback = mine;
		active = true;

		const fail = () => {
			if (!current()) return;
			active = false;
			const reason = mine.started ? 'external-error' : 'external-fallback';
			mine.stop();
			hooks.onError(reason);
		};

		void (async () => {
			try {
				await mine.ready;
				for (const text of chunks) {
					const bytes = await invoke<ArrayBuffer>('tts_external_synthesize', {
						text,
						run: gen,
					});
					if (!current()) return;
					const decoded = decodeExternal(bytes);
					if (!decoded) continue;
					mine.schedule(decoded.samples, decoded.sampleRate);
				}
				mine.finish();
			} catch {
				fail();
			}
		})();
	},
	stop: halt,
};
