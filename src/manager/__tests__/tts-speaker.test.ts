import { speechSynthesisEngine, type TtsEngine } from '../tts/engine';
import { setEngine, speaker } from '../tts/speaker';

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
});
