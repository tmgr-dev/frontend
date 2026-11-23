jest.mock('@tauri-apps/api/core', () => ({ invoke: jest.fn() }));
jest.mock('@tauri-apps/api/event', () => ({ listen: jest.fn() }));

class FakeSource {
	buffer: unknown = null;
	onended: (() => void) | null = null;
	connect = jest.fn();
	start = jest.fn();
	stop = jest.fn(() => this.onended?.());
}

class FakeContext {
	state = 'running';
	currentTime = 0;
	destination = {};
	resume = jest.fn(async () => undefined);
	createBufferSource() {
		return new FakeSource();
	}
	createBuffer(_c: number, length: number) {
		return { duration: 1, getChannelData: () => new Float32Array(length) };
	}
}

const g = globalThis as unknown as Record<string, unknown>;
const flush = () => new Promise((resolve) => setTimeout(resolve, 0));
const hooks = () => ({ onEnd: jest.fn(), onError: jest.fn() });

describe('shared run counter', () => {
	let invokeMock: jest.Mock;
	beforeEach(() => {
		jest.resetModules();
		g.AudioContext = FakeContext;
		invokeMock = (require('@tauri-apps/api/core') as { invoke: jest.Mock })
			.invoke;
		invokeMock.mockReset();
		invokeMock.mockImplementation(async (cmd: string) => {
			if (cmd === 'tts_check') return { ok: true, oov: [] };
			if (cmd === 'tts_synthesize') return new Float32Array(4).buffer;
			if (cmd === 'tts_external_synthesize') {
				const buf = new ArrayBuffer(20);
				new DataView(buf).setUint32(0, 16000, true);
				return buf;
			}
			return undefined;
		});
	});
	afterEach(() => {
		delete g.AudioContext;
	});

	it('nextRun is strictly monotonic', () => {
		const { nextRun } = require('../tts/runCounter') as {
			nextRun: () => number;
		};
		const a = nextRun();
		const b = nextRun();
		expect(b).toBeGreaterThan(a);
	});

	it('run ids rise across alternating engines and a stop never covers the other engine', async () => {
		const { piperEngine } = require('../tts/piperEngine') as {
			piperEngine: import('../tts/engine').TtsEngine;
		};
		const { externalEngine } = require('../tts/externalEngine') as {
			externalEngine: import('../tts/engine').TtsEngine;
		};
		const steps: [string, () => void][] = [
			['p', () => piperEngine.speak(['One.'], 'en', hooks())],
			['stop-p', () => piperEngine.stop()],
			['e', () => externalEngine.speak(['Two.'], 'en', hooks())],
			['stop-e', () => externalEngine.stop()],
			['p', () => piperEngine.speak(['Three.'], 'en', hooks())],
			['stop-p', () => piperEngine.stop()],
			['e', () => externalEngine.speak(['Four.'], 'en', hooks())],
			['stop-e', () => externalEngine.stop()],
		];
		const sent: { kind: string; run: number }[] = [];
		for (const [, step] of steps) {
			const before = invokeMock.mock.calls.length;
			step();
			await flush();
			for (const [cmd, args] of invokeMock.mock.calls.slice(before)) {
				if (args && typeof args.run === 'number')
					sent.push({ kind: cmd, run: args.run });
			}
		}
		const synth = sent.filter((s) => s.kind !== 'tts_stop');
		expect(synth).toHaveLength(4);
		const runs = synth.map((s) => s.run);
		expect([...runs].sort((a, b) => a - b)).toEqual(runs);
		expect(new Set(runs).size).toBe(4);
		const stops = sent.filter((s) => s.kind === 'tts_stop');
		expect(stops).toHaveLength(4);
		stops.forEach((stop, i) => {
			expect(stop.run).toBe(runs[i]);
			if (i + 1 < runs.length) expect(stop.run).toBeLessThan(runs[i + 1]);
		});
	});
});
