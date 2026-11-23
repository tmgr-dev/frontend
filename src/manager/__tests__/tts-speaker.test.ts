import { speechSynthesisEngine, type TtsEngine } from '../tts/engine';
import { piperEngine } from '../tts/piperEngine';
import { setEngine, speaker } from '../tts/speaker';
import { voicePack } from '../tts/voicePack';

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
});
