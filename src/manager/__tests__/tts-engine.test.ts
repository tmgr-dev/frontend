import { speechSynthesisEngine, type TtsHooks } from '../tts/engine';

class FakeUtterance {
	voice: unknown = null;
	lang = '';
	onend: (() => void) | null = null;
	onerror: ((e: { error: string }) => void) | null = null;
	constructor(public text: string) {}
}

const voice = (lang: string, localService?: boolean) =>
	({
		name: `${lang}-${localService}`,
		lang,
		localService,
		default: false,
	}) as unknown as SpeechSynthesisVoice;

const g = globalThis as unknown as Record<string, unknown>;

const install = (voices: SpeechSynthesisVoice[]) => {
	const spoken: FakeUtterance[] = [];
	const synth = {
		getVoices: jest.fn(() => voices),
		speak: jest.fn((u: FakeUtterance) => spoken.push(u)),
		cancel: jest.fn(),
		addEventListener: jest.fn(),
		removeEventListener: jest.fn(),
	};
	g.speechSynthesis = synth;
	g.SpeechSynthesisUtterance = FakeUtterance;
	return { synth, spoken };
};

const hooks = (): TtsHooks & { onEnd: jest.Mock; onError: jest.Mock } => ({
	onEnd: jest.fn(),
	onError: jest.fn(),
});

describe('speechSynthesisEngine', () => {
	afterEach(() => {
		speechSynthesisEngine.stop();
		delete g.speechSynthesis;
		delete g.SpeechSynthesisUtterance;
	});

	it('never chooses a remote voice', () => {
		const { spoken } = install([voice('ru-RU', false), voice('ru-RU', true)]);
		speechSynthesisEngine.speak(['Привет'], 'ru', hooks());
		expect((spoken[0].voice as SpeechSynthesisVoice).localService).toBe(true);
	});

	it('reports no-voice when only remote voices exist for the language', () => {
		const { synth } = install([voice('ru-RU', false), voice('en-US', true)]);
		const h = hooks();
		speechSynthesisEngine.speak(['Привет'], 'ru', h);
		expect(synth.speak).not.toHaveBeenCalled();
		expect(h.onError).toHaveBeenCalledTimes(1);
		expect(h.onError).toHaveBeenCalledWith('no-voice');
	});

	it('calls synth.speak synchronously when voices are already available', () => {
		const { synth } = install([voice('en-US', true)]);
		speechSynthesisEngine.speak(['Hello'], 'en', hooks());
		expect(synth.speak).toHaveBeenCalledTimes(1);
	});

	it('waits for voiceschanged when the voice list starts empty', () => {
		let ready: SpeechSynthesisVoice[] = [];
		const { synth, spoken } = install([]);
		synth.getVoices.mockImplementation(() => ready);
		speechSynthesisEngine.speak(['Hello'], 'en', hooks());
		expect(synth.speak).not.toHaveBeenCalled();
		ready = [voice('en-US', true)];
		const handler = synth.addEventListener.mock.calls[0][1] as () => void;
		handler();
		expect(spoken).toHaveLength(1);
	});

	it('primes the voice list in isAvailable', () => {
		const { synth } = install([voice('en-US', true)]);
		expect(speechSynthesisEngine.isAvailable()).toBe(true);
		expect(synth.getVoices).toHaveBeenCalled();
	});

	it('ends after the last chunk', () => {
		const { spoken } = install([voice('en-US', true)]);
		const h = hooks();
		speechSynthesisEngine.speak(['one', 'two'], 'en', h);
		expect(spoken.map((u) => u.text)).toEqual(['one', 'two']);
		spoken[0].onend?.();
		expect(h.onEnd).not.toHaveBeenCalled();
		spoken[1].onend?.();
		expect(h.onEnd).toHaveBeenCalledTimes(1);
	});

	it('an error on a non-final chunk cancels the queue and is reported once', () => {
		const { synth, spoken } = install([voice('en-US', true)]);
		const h = hooks();
		speechSynthesisEngine.speak(['one', 'two', 'three'], 'en', h);
		spoken[0].onerror?.({ error: 'synthesis-failed' });
		expect(synth.cancel).toHaveBeenCalled();
		spoken[1].onerror?.({ error: 'synthesis-failed' });
		spoken[2].onend?.();
		expect(h.onError).toHaveBeenCalledTimes(1);
		expect(h.onError).toHaveBeenCalledWith('synthesis-failed');
		expect(h.onEnd).not.toHaveBeenCalled();
	});

	it('a cancellation that is not caused by stop() settles as end', () => {
		const { spoken } = install([voice('en-US', true)]);
		const h = hooks();
		speechSynthesisEngine.speak(['one'], 'en', h);
		spoken[0].onerror?.({ error: 'interrupted' });
		expect(h.onEnd).toHaveBeenCalledTimes(1);
		expect(h.onError).not.toHaveBeenCalled();
	});

	it('a cancellation caused by stop() stays silent', () => {
		const { spoken } = install([voice('en-US', true)]);
		const h = hooks();
		speechSynthesisEngine.speak(['one', 'two'], 'en', h);
		speechSynthesisEngine.stop();
		spoken[0].onerror?.({ error: 'canceled' });
		spoken[1].onerror?.({ error: 'canceled' });
		spoken[1].onend?.();
		expect(h.onEnd).not.toHaveBeenCalled();
		expect(h.onError).not.toHaveBeenCalled();
	});
});
