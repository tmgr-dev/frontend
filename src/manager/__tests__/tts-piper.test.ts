import { normalizeEnglish } from '../tts/normalizeEn';

jest.mock('@tauri-apps/api/core', () => ({ invoke: jest.fn() }));
jest.mock('@tauri-apps/api/event', () => ({ listen: jest.fn() }));

type PiperModule = typeof import('../tts/piperEngine');
type PackModule = typeof import('../tts/voicePack');

let piperEngine: PiperModule['piperEngine'];
let voicePack: PackModule['voicePack'];
let invokeMock: jest.Mock;

class FakeSource {
	buffer: { duration: number } | null = null;
	onended: (() => void) | null = null;
	startedAt: number | null = null;
	connect = jest.fn();
	start = jest.fn((when: number) => {
		this.startedAt = when;
	});
	stop = jest.fn(() => this.onended?.());
}

class FakeContext {
	static instances: FakeContext[] = [];
	static failConstruct = false;
	state: 'running' | 'suspended' | 'closed' = 'running';
	currentTime = 0;
	destination = {};
	sources: FakeSource[] = [];
	resume = jest.fn(async () => {
		this.state = 'running';
	});
	constructor() {
		if (FakeContext.failConstruct) throw new Error('no audio');
		FakeContext.instances.push(this);
	}
	createBufferSource() {
		const s = new FakeSource();
		this.sources.push(s);
		return s;
	}
	createBuffer(_channels: number, length: number, rate: number) {
		const data = new Float32Array(length);
		return {
			duration: length / rate,
			getChannelData: () => data,
		};
	}
}

const g = globalThis as unknown as Record<string, unknown>;

const RATE = 4;
const pcm = (samples: number) => new Float32Array(samples).buffer;
const flush = () => new Promise((resolve) => setTimeout(resolve, 0));

const hooks = () => ({ onEnd: jest.fn(), onError: jest.fn() });

const ctx = () => FakeContext.instances[FakeContext.instances.length - 1];

describe('piperEngine', () => {
	beforeEach(() => {
		jest.resetModules();
		invokeMock = (require('@tauri-apps/api/core') as { invoke: jest.Mock })
			.invoke;
		piperEngine = (require('../tts/piperEngine') as PiperModule).piperEngine;
		voicePack = (require('../tts/voicePack') as PackModule).voicePack;
		invokeMock.mockReset();
		FakeContext.instances = [];
		FakeContext.failConstruct = false;
		g.AudioContext = FakeContext;
		voicePack.sampleRate = RATE;
	});

	afterEach(() => {
		piperEngine.stop();
		delete g.AudioContext;
	});

	it('creates the AudioContext synchronously, before any await', () => {
		invokeMock.mockResolvedValue({ ok: true, oov: [] });
		piperEngine.speak(['Hello.'], 'en', hooks());
		expect(FakeContext.instances).toHaveLength(1);
	});

	it('resumes a suspended AudioContext synchronously inside the click', () => {
		invokeMock.mockResolvedValue({ ok: true, oov: [] });
		piperEngine.speak(['Hello.'], 'en', hooks());
		const first = ctx();
		first.state = 'suspended';
		piperEngine.speak(['Again.'], 'en', hooks());
		expect(first.resume).toHaveBeenCalledTimes(1);
		expect(FakeContext.instances).toHaveLength(1);
	});

	it('synthesizes chunks in order and schedules them back to back', async () => {
		invokeMock.mockImplementation(
			async (cmd: string, args: { text: string }) => {
				if (cmd === 'tts_check') return { ok: true, oov: [] };
				if (cmd === 'tts_synthesize')
					return args.text === normalizeEnglish('First chunk.')
						? pcm(4)
						: pcm(2);
				return undefined;
			},
		);
		const h = hooks();
		piperEngine.speak(['First chunk.', 'Second chunk.'], 'en', h);
		await flush();

		expect(invokeMock).toHaveBeenCalledWith('tts_check', {
			chunks: [
				normalizeEnglish('First chunk.'),
				normalizeEnglish('Second chunk.'),
			],
		});
		const synth = invokeMock.mock.calls.filter(([c]) => c === 'tts_synthesize');
		expect(synth.map(([, a]) => a.text)).toEqual([
			normalizeEnglish('First chunk.'),
			normalizeEnglish('Second chunk.'),
		]);
		expect(new Set(synth.map(([, a]) => a.run)).size).toBe(1);

		const [a, b] = ctx().sources;
		expect(a.startedAt).toBe(0);
		expect(b.startedAt).toBe(1);
		expect(h.onEnd).not.toHaveBeenCalled();
		a.onended?.();
		expect(h.onEnd).not.toHaveBeenCalled();
		b.onended?.();
		expect(h.onEnd).toHaveBeenCalledTimes(1);
		expect(h.onError).not.toHaveBeenCalled();
	});

	it('starts generating chunk N+1 before chunk N has finished playing', async () => {
		invokeMock.mockImplementation(async (cmd: string) => {
			if (cmd === 'tts_check') return { ok: true, oov: [] };
			return pcm(4);
		});
		piperEngine.speak(['One.', 'Two.', 'Three.'], 'en', hooks());
		await flush();
		const synth = invokeMock.mock.calls.filter(([c]) => c === 'tts_synthesize');
		expect(synth).toHaveLength(3);
		expect(ctx().sources.map((src) => src.startedAt)).toEqual([0, 1, 2]);
	});

	it('falls back to the OS voice when the check rejects the text', async () => {
		invokeMock.mockResolvedValue({ ok: false, oov: ['zxqv'] });
		const h = hooks();
		piperEngine.speak(['zxqv.'], 'en', h);
		await flush();
		expect(h.onError).toHaveBeenCalledWith('piper-fallback');
		expect(invokeMock).not.toHaveBeenCalledWith(
			'tts_synthesize',
			expect.anything(),
		);
	});

	it('falls back when the check itself fails before audio starts', async () => {
		invokeMock.mockRejectedValue(new Error('backend down'));
		const h = hooks();
		piperEngine.speak(['Hello.'], 'en', h);
		await flush();
		expect(h.onError).toHaveBeenCalledWith('piper-fallback');
		expect(h.onEnd).not.toHaveBeenCalled();
	});

	it('reports piper-error when synthesis fails after audio started', async () => {
		invokeMock.mockImplementation(async (cmd: string) => {
			if (cmd === 'tts_check') return { ok: true, oov: [] };
			if (cmd === 'tts_synthesize' && invokeMock.mock.calls.length > 2)
				throw new Error('model crashed');
			return pcm(4);
		});
		const h = hooks();
		piperEngine.speak(['One.', 'Two.'], 'en', h);
		await flush();
		expect(ctx().sources[0].startedAt).toBe(0);
		expect(h.onError).toHaveBeenCalledWith('piper-error');
		expect(h.onError).toHaveBeenCalledTimes(1);
	});

	it('falls back when the AudioContext cannot be created', () => {
		FakeContext.failConstruct = true;
		const h = hooks();
		piperEngine.speak(['Hello.'], 'en', h);
		expect(h.onError).toHaveBeenCalledWith('piper-fallback');
		expect(invokeMock).not.toHaveBeenCalled();
	});

	it('falls back when resuming the AudioContext fails', async () => {
		invokeMock.mockResolvedValue({ ok: false, oov: [] });
		piperEngine.speak(['Warm up.'], 'en', hooks());
		await flush();
		piperEngine.stop();
		const first = ctx();
		first.state = 'suspended';
		first.resume.mockRejectedValueOnce(new Error('blocked'));
		const h = hooks();
		piperEngine.speak(['Hello.'], 'en', h);
		await flush();
		expect(h.onError).toHaveBeenCalledWith('piper-fallback');
	});

	it('stop discards late synthesis results and tells the backend', async () => {
		let release: (buf: ArrayBuffer) => void = () => {};
		invokeMock.mockImplementation(async (cmd: string) => {
			if (cmd === 'tts_check') return { ok: true, oov: [] };
			if (cmd === 'tts_synthesize')
				return new Promise<ArrayBuffer>((resolve) => {
					release = resolve;
				});
			return undefined;
		});
		const h = hooks();
		piperEngine.speak(['Hello.'], 'en', h);
		await flush();
		piperEngine.stop();
		release(pcm(4));
		await flush();
		expect(ctx().sources).toHaveLength(0);
		expect(invokeMock).toHaveBeenCalledWith('tts_stop');
		expect(h.onEnd).not.toHaveBeenCalled();
		expect(h.onError).not.toHaveBeenCalled();
	});

	it('stop stops already scheduled sources without reporting an end', async () => {
		invokeMock.mockImplementation(async (cmd: string) => {
			if (cmd === 'tts_check') return { ok: true, oov: [] };
			return pcm(4);
		});
		const h = hooks();
		piperEngine.speak(['One.', 'Two.'], 'en', h);
		await flush();
		const [a, b] = ctx().sources;
		piperEngine.stop();
		expect(a.stop).toHaveBeenCalled();
		expect(b.stop).toHaveBeenCalled();
		expect(h.onEnd).not.toHaveBeenCalled();
		expect(h.onError).not.toHaveBeenCalled();
	});

	it('an empty ArrayBuffer ends quietly without an error', async () => {
		invokeMock.mockImplementation(async (cmd: string) => {
			if (cmd === 'tts_check') return { ok: true, oov: [] };
			return new ArrayBuffer(0);
		});
		const h = hooks();
		piperEngine.speak(['Hello.'], 'en', h);
		await flush();
		expect(h.onEnd).toHaveBeenCalledTimes(1);
		expect(h.onError).not.toHaveBeenCalled();
	});
});
