import { canOpenInApp, writeClipboard } from '../share';

class FakeClipboardItem {
	constructor(public items: Record<string, Promise<Blob>>) {}
}

const setClipboard = (
	clipboard: Partial<Clipboard> | undefined,
	userAgent = 'node',
) =>
	Object.defineProperty(globalThis, 'navigator', {
		value: { clipboard, userAgent },
		configurable: true,
	});

describe('writeClipboard', () => {
	afterEach(() => {
		delete (globalThis as any).ClipboardItem;
	});

	it('writes a string with writeText', async () => {
		const writeText = jest.fn().mockResolvedValue(undefined);
		setClipboard({ writeText });
		await writeClipboard('hello');
		expect(writeText).toHaveBeenCalledWith('hello');
	});

	it('starts a ClipboardItem write before the text resolves', async () => {
		(globalThis as any).ClipboardItem = FakeClipboardItem;
		const write = jest.fn(async (items: FakeClipboardItem[]) => {
			await items[0].items['text/plain'];
		});
		setClipboard({ write: write as any, writeText: jest.fn() });
		let resolve: (text: string) => void = () => undefined;
		const done = writeClipboard(new Promise((r) => (resolve = r)));
		expect(write).toHaveBeenCalledTimes(1);
		resolve('# Page');
		await done;
		const blob = await write.mock.calls[0][0][0].items['text/plain'];
		expect(await blob.text()).toBe('# Page');
	});

	it('rejects when the text fails to load, without an unhandled rejection', async () => {
		(globalThis as any).ClipboardItem = FakeClipboardItem;
		const write = jest.fn(async () => {
			throw new Error('denied');
		});
		setClipboard({ write: write as any, writeText: jest.fn() });
		const unhandled = jest.fn();
		process.on('unhandledRejection', unhandled);
		await expect(
			writeClipboard(Promise.reject(new Error('offline'))),
		).rejects.toThrow('denied');
		await new Promise((r) => setTimeout(r, 0));
		process.off('unhandledRejection', unhandled);
		expect(unhandled).not.toHaveBeenCalled();
	});

	it('falls back to writeText after the text resolves', async () => {
		const writeText = jest.fn().mockResolvedValue(undefined);
		setClipboard({ writeText });
		await writeClipboard(Promise.resolve('later'));
		expect(writeText).toHaveBeenCalledWith('later');
	});

	it('rejects when the clipboard is unavailable', async () => {
		setClipboard(undefined);
		await expect(writeClipboard('x')).rejects.toThrow();
	});

	it('rejects without an unhandled rejection when the clipboard is unavailable and the text fails', async () => {
		setClipboard(undefined);
		const unhandled = jest.fn();
		process.on('unhandledRejection', unhandled);
		await expect(
			writeClipboard(Promise.reject(new Error('404'))),
		).rejects.toThrow('Clipboard not supported');
		await new Promise((resolve) => setTimeout(resolve, 0));
		process.off('unhandledRejection', unhandled);
		expect(unhandled).not.toHaveBeenCalled();
	});

	it('rejects without a sync throw when writeText is denied', async () => {
		setClipboard({
			writeText: jest.fn().mockRejectedValue(new Error('denied')),
		});
		let result: Promise<void> | undefined;
		expect(() => (result = writeClipboard('x'))).not.toThrow();
		await expect(result).rejects.toThrow('denied');
	});
});

describe('canOpenInApp', () => {
	it('is offered in a desktop browser and not on mobile', () => {
		setClipboard(undefined, 'Mozilla/5.0 (Macintosh) Chrome/130');
		expect(canOpenInApp()).toBe(true);
		setClipboard(undefined, 'Mozilla/5.0 (iPhone) Mobile Safari');
		expect(canOpenInApp()).toBe(false);
		setClipboard(undefined, 'Mozilla/5.0 (Linux; Android 14) Chrome/130');
		expect(canOpenInApp()).toBe(false);
	});
});
