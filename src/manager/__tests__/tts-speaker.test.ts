import { speechSynthesisEngine, type TtsEngine } from '../tts/engine';
import { externalEngine } from '../tts/externalEngine';
import { externalVoice } from '../tts/externalVoice';
import { piperEngine } from '../tts/piperEngine';
import { setEngine, speaker } from '../tts/speaker';
import { voicePack, voicePacks } from '../tts/voicePack';

jest.mock('@tauri-apps/api/core', () => ({ invoke: jest.fn() }));
jest.mock('@tauri-apps/api/event', () => ({ listen: jest.fn() }));
jest.mock('../tts/piperEngine', () => ({
	piperEngine: {
		id: 'piper',
		isAvailable: () => true,
		speak: jest.fn(),
		stop: jest.fn(),
	},
}));

jest.mock('../tts/externalEngine', () => ({
	externalEngine: {
		id: 'external',
		isAvailable: () => true,
		speak: jest.fn(),
		stop: jest.fn(),
	},
}));

const external = externalEngine as unknown as {
	speak: jest.Mock;
	stop: jest.Mock;
};

const piper = piperEngine as unknown as {
	speak: jest.Mock;
	stop: jest.Mock;
};

type Hooks = { onEnd(): void; onError(reason: string): void };
type Call = { chunks: string[]; lang: string; hooks: Hooks };

const makeFake = (available = true) => {
	const calls: Call[] = [];
	const engine: TtsEngine = {
		id: 'fake',
		isAvailable: () => available,
		speak: (chunks, lang, hooks) => {
			calls.push({ chunks, lang, hooks });
		},
		stop: jest.fn(),
	};
	return { engine, calls, last: () => calls[calls.length - 1] };
};

describe('speaker', () => {
	afterEach(() => setEngine(speechSynthesisEngine));

	it('plays one message at a time and starting another stops the current one', () => {
		const fake = makeFake();
		setEngine(fake.engine);
		speaker.toggle('a', 'Hello there.');
		expect(speaker.speakingId).toBe('a');
		expect(fake.calls[0]).toMatchObject({
			chunks: ['Hello there.'],
			lang: 'en',
		});

		const first = fake.last();
		speaker.toggle('b', 'Second message.');
		expect(fake.engine.stop).toHaveBeenCalled();
		expect(speaker.speakingId).toBe('b');
		expect(fake.calls).toHaveLength(2);

		first.hooks.onEnd();
		expect(speaker.speakingId).toBe('b');
	});

	it('toggling the same message stops it', () => {
		const fake = makeFake();
		setEngine(fake.engine);
		speaker.toggle('a', 'Hello.');
		speaker.toggle('a', 'Hello.');
		expect(speaker.speakingId).toBeNull();
		expect(fake.engine.stop).toHaveBeenCalled();
		expect(fake.calls).toHaveLength(1);
	});

	it('clears speakingId when the engine reports the end', () => {
		const fake = makeFake();
		setEngine(fake.engine);
		speaker.toggle('a', 'Hello.');
		fake.last().hooks.onEnd();
		expect(speaker.speakingId).toBeNull();
		expect(speaker.error).toBeNull();
	});

	it('maps no-voice to an error for that message', () => {
		const fake = makeFake();
		setEngine(fake.engine);
		speaker.toggle('a', 'Привет.');
		fake.last().hooks.onError('no-voice');
		expect(speaker.error).toBe('no-voice');
		expect(speaker.errorId).toBe('a');
		expect(speaker.speakingId).toBeNull();
	});

	it('treats a cancelled utterance as a normal stop, not an error', () => {
		const fake = makeFake();
		setEngine(fake.engine);
		speaker.toggle('a', 'Hello.');
		fake.last().hooks.onError('canceled');
		expect(speaker.error).toBeNull();
		expect(speaker.errorId).toBeNull();
		expect(speaker.speakingId).toBeNull();
	});

	it('clears the previous error when a message is played again', () => {
		const fake = makeFake();
		setEngine(fake.engine);
		speaker.toggle('a', 'Hello.');
		fake.last().hooks.onError('no-voice');
		speaker.toggle('a', 'Hello.');
		expect(speaker.error).toBeNull();
		expect(speaker.speakingId).toBe('a');
	});

	it('ignores events from a run that was already replaced', () => {
		const fake = makeFake();
		setEngine(fake.engine);
		speaker.toggle('a', 'Hello.');
		const stale = fake.last();
		speaker.toggle('b', 'Bye.');
		stale.hooks.onError('no-voice');
		expect(speaker.error).toBeNull();
		expect(speaker.speakingId).toBe('b');
	});

	it('does nothing for an empty body', () => {
		const fake = makeFake();
		setEngine(fake.engine);
		speaker.toggle('a', '   ');
		expect(fake.calls).toHaveLength(0);
		expect(speaker.speakingId).toBeNull();
	});

	it('reports unavailable and does not speak when the engine is missing', () => {
		const fake = makeFake(false);
		setEngine(fake.engine);
		expect(speaker.available).toBe(false);
		speaker.toggle('a', 'Hello.');
		expect(fake.calls).toHaveLength(0);
		expect(speaker.speakingId).toBeNull();
	});

	describe('Piper voice selection', () => {
		const piperHooks = () =>
			piper.speak.mock.calls[piper.speak.mock.calls.length - 1][2];
		beforeEach(() => {
			piper.speak.mockReset();
			piper.stop.mockReset();
			voicePack.supported = true;
			voicePack.installed = true;
		});
		afterEach(() => {
			voicePack.supported = false;
			voicePack.installed = false;
		});

		it('uses the Piper voice for English when the pack is installed', () => {
			const fake = makeFake();
			setEngine(fake.engine);
			speaker.toggle('a', 'Hello there.');
			expect(piper.speak).toHaveBeenCalledWith(
				['Hello there.'],
				'en',
				expect.any(Object),
			);
			expect(fake.calls).toHaveLength(0);
			expect(speaker.speakingId).toBe('a');
		});

		it('is available through Piper when the OS voice is not', () => {
			setEngine(makeFake(false).engine);
			expect(speaker.available).toBe(true);
			voicePack.installed = false;
			expect(speaker.available).toBe(false);
		});

		it('keeps the OS voice for Russian even when the pack is installed', () => {
			const fake = makeFake();
			setEngine(fake.engine);
			speaker.toggle('a', 'Привет.');
			expect(piper.speak).not.toHaveBeenCalled();
			expect(fake.calls[0]).toMatchObject({ lang: 'ru' });
		});

		it('uses the OS voice for English when the pack is not installed', () => {
			voicePack.installed = false;
			const fake = makeFake();
			setEngine(fake.engine);
			speaker.toggle('a', 'Hello.');
			expect(piper.speak).not.toHaveBeenCalled();
			expect(fake.calls).toHaveLength(1);
		});

		it('retries the same run with the OS voice on piper-fallback', () => {
			const fake = makeFake();
			setEngine(fake.engine);
			speaker.toggle('a', 'Hello there.');
			piperHooks().onError('piper-fallback');
			expect(fake.calls).toHaveLength(1);
			expect(fake.calls[0]).toMatchObject({
				chunks: ['Hello there.'],
				lang: 'en',
			});
			expect(speaker.speakingId).toBe('a');
			expect(speaker.error).toBeNull();
			fake.last().hooks.onEnd();
			expect(speaker.speakingId).toBeNull();
		});

		it('ignores a fallback from a run that was already replaced', () => {
			const fake = makeFake();
			setEngine(fake.engine);
			speaker.toggle('a', 'Hello.');
			const stale = piperHooks();
			speaker.toggle('b', 'Привет.');
			stale.onError('piper-fallback');
			expect(fake.calls.map((c) => c.chunks[0])).toEqual(['Привет.']);
			expect(speaker.speakingId).toBe('b');
		});

		it('reports a piper-error for that message without falling back', () => {
			const fake = makeFake();
			setEngine(fake.engine);
			speaker.toggle('a', 'Hello.');
			piperHooks().onError('piper-error');
			expect(fake.calls).toHaveLength(0);
			expect(speaker.error).toBe('piper-error');
			expect(speaker.errorId).toBe('a');
			expect(speaker.speakingId).toBeNull();
		});

		it('stop also stops the Piper engine', () => {
			const fake = makeFake();
			setEngine(fake.engine);
			speaker.toggle('a', 'Hello.');
			speaker.stop();
			expect(piper.stop).toHaveBeenCalled();
			expect(speaker.speakingId).toBeNull();
		});
	});

	describe('engine order', () => {
		const lastHooks = (m: jest.Mock) =>
			m.mock.calls[m.mock.calls.length - 1][2];
		const setHealth = (over: Record<string, unknown> = {}) => {
			externalVoice.health = {
				configured: true,
				mode: 'http',
				healthy: true,
				lastError: null,
				...over,
			};
		};
		beforeEach(() => {
			external.speak.mockReset();
			external.stop.mockReset();
			piper.speak.mockReset();
			piper.stop.mockReset();
			setHealth();
			voicePacks.en.supported = true;
			voicePacks.en.installed = true;
			voicePacks.ru.supported = true;
			voicePacks.ru.installed = true;
		});
		afterEach(() => {
			jest.restoreAllMocks();
			externalVoice.health = null;
			voicePacks.en.supported = false;
			voicePacks.en.installed = false;
			voicePacks.ru.supported = false;
			voicePacks.ru.installed = false;
		});

		it('prefers the external voice over Piper and the OS voice', () => {
			const fake = makeFake();
			setEngine(fake.engine);
			speaker.toggle('a', 'Привет.');
			expect(external.speak).toHaveBeenCalledWith(
				['Привет.'],
				'ru',
				expect.any(Object),
			);
			expect(piper.speak).not.toHaveBeenCalled();
			expect(fake.calls).toHaveLength(0);
		});

		it('skips the external voice when it is not usable', () => {
			for (const over of [
				{ healthy: false },
				{ configured: false },
				{ mode: 'off' },
			]) {
				setHealth(over);
				piper.speak.mockReset();
				speaker.toggle('a', 'Hello.');
				expect(external.speak).not.toHaveBeenCalled();
				expect(piper.speak).toHaveBeenCalledTimes(1);
				speaker.stop();
			}
		});

		it('uses the Piper pack of the message language', () => {
			setHealth({ healthy: false });
			voicePacks.ru.installed = false;
			const fake = makeFake();
			setEngine(fake.engine);
			speaker.toggle('a', 'Привет.');
			expect(piper.speak).not.toHaveBeenCalled();
			expect(fake.calls[0]).toMatchObject({ lang: 'ru' });
			speaker.toggle('b', 'Hello.');
			expect(piper.speak).toHaveBeenCalledWith(
				['Hello.'],
				'en',
				expect.any(Object),
			);
		});

		it('uses the Russian Piper pack for Russian when installed', () => {
			setHealth({ healthy: false });
			speaker.toggle('a', 'Привет.');
			expect(piper.speak).toHaveBeenCalledWith(
				['Привет.'],
				'ru',
				expect.any(Object),
			);
		});

		it('falls through external, Piper, OS when nothing has played yet', () => {
			const fake = makeFake();
			setEngine(fake.engine);
			speaker.toggle('a', 'Hello there.');
			lastHooks(external.speak).onError('external-fallback');
			expect(piper.speak).toHaveBeenCalledTimes(1);
			expect(speaker.error).toBeNull();
			expect(speaker.speakingId).toBe('a');
			lastHooks(piper.speak).onError('piper-fallback');
			expect(fake.calls).toHaveLength(1);
			expect(speaker.error).toBeNull();
			fake.last().hooks.onEnd();
			expect(speaker.speakingId).toBeNull();
		});

		it('never switches voice after external audio started and shows the error', () => {
			const fake = makeFake();
			setEngine(fake.engine);
			speaker.toggle('a', 'Hello there.');
			lastHooks(external.speak).onError('external-error');
			expect(piper.speak).not.toHaveBeenCalled();
			expect(fake.calls).toHaveLength(0);
			expect(speaker.error).toBe('external-error');
			expect(speaker.errorId).toBe('a');
			expect(speaker.speakingId).toBeNull();
		});

		it('marks the external voice unhealthy after a failure so the next message skips it', () => {
			const fake = makeFake();
			setEngine(fake.engine);
			speaker.toggle('a', 'Hello there.');
			lastHooks(external.speak).onError('external-fallback');
			expect(externalVoice.usable).toBe(false);
			external.speak.mockReset();
			speaker.toggle('b', 'Next one.');
			expect(external.speak).not.toHaveBeenCalled();
			expect(piper.speak).toHaveBeenCalledTimes(2);
		});

		it('surfaces the failure when no other engine can speak', () => {
			setEngine(makeFake(false).engine);
			voicePacks.en.installed = false;
			speaker.toggle('a', 'Hello there.');
			lastHooks(external.speak).onError('external-fallback');
			expect(speaker.error).toBe('external-fallback');
			expect(speaker.errorId).toBe('a');
			expect(speaker.speakingId).toBeNull();
		});

		it('a stop or cancel from an engine is not an error', () => {
			speaker.toggle('a', 'Hello there.');
			lastHooks(external.speak).onError('canceled');
			expect(speaker.error).toBeNull();
			expect(speaker.speakingId).toBeNull();
		});

		it('ignores a fallback from a replaced run', () => {
			const fake = makeFake();
			setEngine(fake.engine);
			speaker.toggle('a', 'Hello there.');
			const stale = lastHooks(external.speak);
			speaker.toggle('b', 'Other.');
			piper.speak.mockReset();
			stale.onError('external-fallback');
			expect(piper.speak).not.toHaveBeenCalled();
			expect(speaker.speakingId).toBe('b');
		});

		it('pressing the same message again stops every engine', () => {
			speaker.toggle('a', 'Hello there.');
			speaker.toggle('a', 'Hello there.');
			expect(external.stop).toHaveBeenCalled();
			expect(piper.stop).toHaveBeenCalled();
			expect(speaker.speakingId).toBeNull();
			expect(external.speak).toHaveBeenCalledTimes(1);
		});

		it('is available when only the external voice or a pack can speak', () => {
			setEngine(makeFake(false).engine);
			voicePacks.en.installed = false;
			voicePacks.ru.installed = false;
			expect(speaker.available).toBe(true);
			setHealth({ healthy: false });
			expect(speaker.available).toBe(false);
			voicePacks.ru.installed = true;
			expect(speaker.available).toBe(true);
		});
	});
});
