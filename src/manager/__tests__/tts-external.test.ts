jest.mock('@tauri-apps/api/core', () => ({ invoke: jest.fn() }));
jest.mock('@tauri-apps/api/event', () => ({ listen: jest.fn() }));

type ExternalModule = typeof import('../tts/externalEngine');

let externalEngine: ExternalModule['externalEngine'];
let invokeMock: jest.Mock;

class FakeSource {
	buffer: { duration: number; rate: number } | null = null;
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
	state: 'running' | 'suspended' | 'closed' = 'running';
	currentTime = 0;
	destination = {};
	sources: FakeSource[] = [];
	resume = jest.fn(async () => {
		this.state = 'running';
	});
	constructor() {
		FakeContext.instances.push(this);
	}
	createBufferSource() {
		const s = new FakeSource();
		this.sources.push(s);
		return s;
	}
	createBuffer(_channels: number, length: number, rate: number) {
		const data = new Float32Array(length);
		return { duration: length / rate, rate, getChannelData: () => data };
	}
}

const g = globalThis as unknown as Record<string, unknown>;

const payload = (rate: number, samples: number) => {
	const buf = new ArrayBuffer(4 + samples * 4);
	new DataView(buf).setUint32(0, rate, true);
	new Float32Array(buf, 4).fill(0.5);
	return buf;
};
const flush = () => new Promise((resolve) => setTimeout(resolve, 0));
const hooks = () => ({ onEnd: jest.fn(), onError: jest.fn() });
const ctx = () => FakeContext.instances[FakeContext.instances.length - 1];

describe('externalEngine', () => {
	beforeEach(() => {
		jest.resetModules();
		invokeMock = (require('@tauri-apps/api/core') as { invoke: jest.Mock })
			.invoke;
		externalEngine = (require('../tts/externalEngine') as ExternalModule)
			.externalEngine;
		invokeMock.mockReset();
		FakeContext.instances = [];
		g.AudioContext = FakeContext;
	});

	afterEach(() => {
		externalEngine.stop();
		delete g.AudioContext;
	});

	it('creates the AudioContext synchronously and resumes a suspended one', () => {
		invokeMock.mockResolvedValue(payload(4, 4));
		externalEngine.speak(['Hello.'], 'en', hooks());
		expect(FakeContext.instances).toHaveLength(1);
		ctx().state = 'suspended';
		externalEngine.speak(['Again.'], 'ru', hooks());
		expect(ctx().resume).toHaveBeenCalledTimes(1);
		expect(FakeContext.instances).toHaveLength(1);
	});

	it('reads the sample rate header and schedules chunks back to back', async () => {
		invokeMock.mockImplementation(async (_c: string, a: { text: string }) =>
			a.text === 'One.' ? payload(4, 4) : payload(8, 4),
		);
		const h = hooks();
		externalEngine.speak(['One.', 'Two.'], 'ru', h);
		await flush();
		expect(invokeMock).toHaveBeenCalledWith('tts_external_synthesize', {
			text: 'One.',
			run: expect.any(Number),
		});
		const [a, b] = ctx().sources;
		expect(a.buffer?.rate).toBe(4);
		expect(b.buffer?.rate).toBe(8);
		expect(a.startedAt).toBe(0);
		expect(b.startedAt).toBe(1);
		a.onended?.();
		expect(h.onEnd).not.toHaveBeenCalled();
		b.onended?.();
		expect(h.onEnd).toHaveBeenCalledTimes(1);
		expect(h.onError).not.toHaveBeenCalled();
	});

	it('skips an empty chunk and continues with the next one', async () => {
		let n = 0;
		invokeMock.mockImplementation(async () =>
			n++ === 0 ? new ArrayBuffer(0) : payload(4, 4),
		);
		const h = hooks();
		externalEngine.speak(['...', 'Hello.'], 'en', h);
		await flush();
		expect(ctx().sources).toHaveLength(1);
	});

	it('reports external-fallback when nothing has played yet', async () => {
		invokeMock.mockRejectedValue('server down');
		const h = hooks();
		externalEngine.speak(['Hello.'], 'en', h);
		await flush();
		expect(h.onError).toHaveBeenCalledWith('external-fallback');
		expect(h.onEnd).not.toHaveBeenCalled();
	});

	it('reports external-error after audio started and stops playback', async () => {
		let n = 0;
		invokeMock.mockImplementation(async () => {
			if (n++ === 0) return payload(4, 4);
			throw new Error('boom');
		});
		const h = hooks();
		externalEngine.speak(['One.', 'Two.'], 'en', h);
		await flush();
		expect(h.onError).toHaveBeenCalledWith('external-error');
		expect(ctx().sources[0].stop).toHaveBeenCalled();
	});

	it('treats a malformed payload as a failure', async () => {
		invokeMock.mockResolvedValue(new ArrayBuffer(7));
		const h = hooks();
		externalEngine.speak(['Hello.'], 'en', h);
		await flush();
		expect(h.onError).toHaveBeenCalledWith('external-fallback');
	});

	it('a stop is never an error and stops the backend run', async () => {
		let resolve: (b: ArrayBuffer) => void = () => undefined;
		invokeMock.mockImplementation((cmd: string) =>
			cmd === 'tts_stop'
				? Promise.resolve()
				: new Promise<ArrayBuffer>((r) => (resolve = r)),
		);
		const h = hooks();
		externalEngine.speak(['Hello.'], 'en', h);
		await flush();
		externalEngine.stop();
		resolve(payload(4, 4));
		await flush();
		expect(invokeMock).toHaveBeenCalledWith('tts_stop', {
			run: expect.any(Number),
		});
		expect(h.onError).not.toHaveBeenCalled();
		expect(h.onEnd).not.toHaveBeenCalled();
		expect(ctx().sources).toHaveLength(0);
	});

	it('normalises text for the language and drops chunks that become empty', async () => {
		invokeMock.mockResolvedValue(payload(4, 4));
		externalEngine.speak(
			['Fix 5 bugs & ship', '   ', 'Готово на 5'],
			'en',
			hooks(),
		);
		await flush();
		const texts = invokeMock.mock.calls.map((c) => c[1].text);
		expect(texts).toEqual(['Fix five bugs and ship', 'Готово на five']);
		invokeMock.mockClear();
		externalEngine.speak(['Готово на 5'], 'ru', hooks());
		await flush();
		expect(
			invokeMock.mock.calls
				.filter((c) => c[0] === 'tts_external_synthesize')
				.map((c) => c[1].text),
		).toEqual(['Готово на пять']);
	});

	it('ends at once when every chunk normalises to nothing', async () => {
		const h = hooks();
		externalEngine.speak(['  ', ''], 'en', h);
		await flush();
		expect(h.onEnd).toHaveBeenCalledTimes(1);
		expect(invokeMock).not.toHaveBeenCalled();
	});
});
