jest.mock('@tauri-apps/api/core', () => ({ invoke: jest.fn() }));
jest.mock('@tauri-apps/api/event', () => ({ listen: jest.fn() }));

let invokeMock: jest.Mock;
let listenMock: jest.Mock;

const status = (over: Record<string, unknown> = {}) => ({
	supported: true,
	installed: false,
	installing: false,
	sizeBytes: 94 * 1024 * 1024,
	sampleRate: 22050,
	...over,
});

const flush = () => new Promise((resolve) => setTimeout(resolve, 0));

const loadPack = () => require('../tts/voicePack') as PackModule;
type PackModule = typeof import('../tts/voicePack');

describe('voicePack', () => {
	let progressHandler: ((e: { payload: unknown }) => void) | null;
	let unlisten: jest.Mock;

	beforeEach(() => {
		jest.resetModules();
		invokeMock = (require('@tauri-apps/api/core') as { invoke: jest.Mock })
			.invoke;
		listenMock = (require('@tauri-apps/api/event') as { listen: jest.Mock })
			.listen;
		invokeMock.mockReset();
		listenMock.mockReset();
		progressHandler = null;
		unlisten = jest.fn();
		listenMock.mockImplementation(async (_event: string, handler) => {
			progressHandler = handler;
			return unlisten;
		});
	});

	it('refresh copies the backend status', async () => {
		invokeMock.mockResolvedValue(status({ installed: true }));
		const pack = loadPack().voicePack;
		await pack.refresh();
		expect(invokeMock).toHaveBeenCalledWith('tts_status');
		expect(pack).toMatchObject({
			supported: true,
			installed: true,
			installing: false,
			sizeBytes: 94 * 1024 * 1024,
			sampleRate: 22050,
		});
	});

	it('treats a failing status call as unsupported', async () => {
		invokeMock.mockRejectedValue(new Error('no backend'));
		const pack = loadPack().voicePack;
		pack.supported = true;
		pack.installed = true;
		await pack.refresh();
		expect(pack.supported).toBe(false);
		expect(pack.installed).toBe(false);
	});

	it('install shows progress from events and refreshes the status when done', async () => {
		const pack = loadPack().voicePack;
		invokeMock.mockImplementation(async (cmd: string) => {
			if (cmd === 'tts_install') {
				progressHandler?.({ payload: { received: 50, total: 200 } });
				expect(pack.installing).toBe(true);
				expect(pack.progress).toBe(0.25);
				return undefined;
			}
			return status({ installed: true });
		});
		await pack.install();
		expect(listenMock).toHaveBeenCalledWith(
			'manager://tts-progress',
			expect.any(Function),
		);
		expect(pack.installed).toBe(true);
		expect(pack.installing).toBe(false);
		expect(pack.error).toBeNull();
		expect(unlisten).toHaveBeenCalled();
	});

	it('ignores progress events with an unknown total', async () => {
		const pack = loadPack().voicePack;
		invokeMock.mockImplementation(async (cmd: string) => {
			if (cmd === 'tts_install') {
				progressHandler?.({ payload: { received: 10, total: 0 } });
				return undefined;
			}
			return status({ installed: true });
		});
		await pack.install();
		expect(pack.progress).toBe(0);
	});

	it('records a failed install as an error and leaves the pack uninstalled', async () => {
		const pack = loadPack().voicePack;
		invokeMock.mockImplementation(async (cmd: string) => {
			if (cmd === 'tts_install') throw 'disk full';
			return status();
		});
		await pack.install();
		expect(pack.error).toBe('disk full');
		expect(pack.installed).toBe(false);
		expect(pack.installing).toBe(false);
	});

	it('a cancelled install is not an error', async () => {
		const pack = loadPack().voicePack;
		invokeMock.mockImplementation(async (cmd: string) => {
			if (cmd === 'tts_install') throw 'cancelled';
			return status();
		});
		await pack.install();
		expect(pack.error).toBeNull();
	});

	it('cancel and remove call the backend and refresh the status', async () => {
		const pack = loadPack().voicePack;
		invokeMock.mockImplementation(async (cmd: string) => {
			if (cmd === 'tts_status') return status({ installed: false });
			return undefined;
		});
		pack.installed = true;
		await pack.remove();
		expect(invokeMock).toHaveBeenCalledWith('tts_remove');
		expect(pack.installed).toBe(false);

		await pack.cancel();
		expect(invokeMock).toHaveBeenCalledWith('tts_cancel_install');
	});

	it('does not start a second install while one is running', async () => {
		const pack = loadPack().voicePack;
		let finish: () => void = () => {};
		invokeMock.mockImplementation((cmd: string) => {
			if (cmd === 'tts_install')
				return new Promise<void>((resolve) => {
					finish = resolve;
				});
			return Promise.resolve(status());
		});
		const first = pack.install();
		await flush();
		void pack.install();
		expect(
			invokeMock.mock.calls.filter(([c]) => c === 'tts_install'),
		).toHaveLength(1);
		finish();
		await first;
	});
});
