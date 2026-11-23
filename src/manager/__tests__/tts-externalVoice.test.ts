jest.mock('@tauri-apps/api/core', () => ({ invoke: jest.fn() }));
jest.mock('@tauri-apps/api/event', () => ({ listen: jest.fn() }));

type VoiceModule = typeof import('../tts/externalVoice');

let externalVoice: VoiceModule['externalVoice'];
let invokeMock: jest.Mock;

class FakeSource {
	onended: (() => void) | null = null;
	connect = jest.fn();
	start = jest.fn();
	stop = jest.fn();
	buffer: unknown = null;
}

class FakeContext {
	static instances: FakeContext[] = [];
	state = 'running';
	currentTime = 0;
	destination = {};
	sources: FakeSource[] = [];
	resume = jest.fn(async () => undefined);
	constructor() {
		FakeContext.instances.push(this);
	}
	createBufferSource() {
		const s = new FakeSource();
		this.sources.push(s);
		return s;
	}
	createBuffer(_c: number, length: number, rate: number) {
		return {
			duration: length / rate,
			getChannelData: () => new Float32Array(length),
		};
	}
}

const g = globalThis as unknown as Record<string, unknown>;
const flush = () => new Promise((resolve) => setTimeout(resolve, 0));

const settings = (mode: 'off' | 'http' | 'command' = 'http') => ({
	mode,
	http: {
		baseUrl: 'http://127.0.0.1:8000',
		model: 'm',
		voice: 'v',
		allowRemote: false,
	},
	command: { argv: ['say'], timeoutSecs: 20 },
});
const health = (over: Record<string, unknown> = {}) => ({
	configured: true,
	mode: 'http',
	healthy: true,
	lastError: null,
	...over,
});
const payload = () => {
	const buf = new ArrayBuffer(8);
	new DataView(buf).setUint32(0, 16000, true);
	return buf;
};

describe('externalVoice', () => {
	beforeEach(() => {
		jest.resetModules();
		invokeMock = (require('@tauri-apps/api/core') as { invoke: jest.Mock })
			.invoke;
		externalVoice = (require('../tts/externalVoice') as VoiceModule)
			.externalVoice;
		invokeMock.mockReset();
		FakeContext.instances = [];
		g.AudioContext = FakeContext;
	});
	afterEach(() => {
		delete g.AudioContext;
	});

	it('loads settings and health', async () => {
		invokeMock.mockImplementation(async (cmd: string) =>
			cmd === 'tts_settings_get' ? settings() : health(),
		);
		await externalVoice.load();
		expect(externalVoice.settings.http.baseUrl).toBe('http://127.0.0.1:8000');
		expect(externalVoice.health?.healthy).toBe(true);
		expect(externalVoice.loading).toBe(false);
		expect(externalVoice.usable).toBe(true);
	});

	it('is usable only when mode is on, configured and healthy', async () => {
		for (const [h, usable] of [
			[health({ mode: 'off' }), false],
			[health({ configured: false }), false],
			[health({ healthy: false }), false],
			[health(), true],
		] as const) {
			invokeMock.mockResolvedValue(h);
			await externalVoice.refreshHealth();
			expect(externalVoice.usable).toBe(usable);
		}
		invokeMock.mockRejectedValue('x');
		await externalVoice.refreshHealth();
		expect(externalVoice.usable).toBe(false);
	});

	it('save stores the returned settings and refreshes health', async () => {
		invokeMock.mockImplementation(async (cmd: string) =>
			cmd === 'tts_settings_set'
				? settings('command')
				: health({ mode: 'command' }),
		);
		const ok = await externalVoice.save(settings('command'));
		expect(ok).toBe(true);
		expect(invokeMock).toHaveBeenCalledWith('tts_settings_set', {
			settings: settings('command'),
		});
		expect(externalVoice.settings.mode).toBe('command');
		expect(externalVoice.health?.mode).toBe('command');
		expect(externalVoice.saveError).toBeNull();
	});

	it('save shows nothing when the user cancels the native confirmation', async () => {
		invokeMock.mockRejectedValue('cancelled');
		expect(await externalVoice.save(settings())).toBe(false);
		expect(externalVoice.saveError).toBeNull();
		expect(externalVoice.saving).toBe(false);
	});

	it('save shows other errors', async () => {
		invokeMock.mockRejectedValue('base url is not local');
		expect(await externalVoice.save(settings())).toBe(false);
		expect(externalVoice.saveError).toBe('base url is not local');
	});

	it('test plays the returned audio and refreshes health', async () => {
		invokeMock.mockImplementation(async (cmd: string) =>
			cmd === 'tts_external_test' ? payload() : health(),
		);
		const done = externalVoice.test();
		expect(FakeContext.instances).toHaveLength(1);
		await done;
		const ctx = FakeContext.instances[0];
		expect(ctx.sources).toHaveLength(1);
		expect(invokeMock).toHaveBeenCalledWith('tts_external_health');
		expect(externalVoice.testing).toBe(true);
		ctx.sources[0].onended?.();
		expect(externalVoice.testing).toBe(false);
		expect(externalVoice.testError).toBeNull();
	});

	it('test sends the phrase to the backend', async () => {
		invokeMock.mockImplementation(async (cmd: string) =>
			cmd === 'tts_external_test' ? payload() : health(),
		);
		await externalVoice.test('Hello there.');
		expect(invokeMock).toHaveBeenCalledWith('tts_external_test', {
			text: 'Hello there.',
		});
	});

	it('test shows the backend error text and refreshes health', async () => {
		invokeMock.mockImplementation(async (cmd: string) => {
			if (cmd === 'tts_external_test') throw 'connection refused';
			return health({ healthy: false, lastError: 'connection refused' });
		});
		await externalVoice.test();
		await flush();
		expect(externalVoice.testError).toBe('connection refused');
		expect(externalVoice.testing).toBe(false);
		expect(externalVoice.health?.lastError).toBe('connection refused');
		expect(externalVoice.usable).toBe(false);
	});

	it('markUnhealthy flips usable immediately', async () => {
		invokeMock.mockResolvedValue(health());
		await externalVoice.refreshHealth();
		invokeMock.mockReturnValue(new Promise(() => undefined));
		externalVoice.markUnhealthy();
		expect(externalVoice.usable).toBe(false);
	});
});
