import { createLlmChat } from '../llmClient';

type Handler = (e: { payload: { request_id: string; event: any } }) => void;

let handler: Handler | null = null;
const mockInvoke = jest.fn();

jest.mock('@tauri-apps/api/core', () => ({
	invoke: (...args: unknown[]) => mockInvoke(...args),
}));
jest.mock('@tauri-apps/api/event', () => ({
	listen: async (_name: string, fn: Handler) => {
		handler = fn;
		return () => {
			handler = null;
		};
	},
}));

const emit = (requestId: string, event: unknown) =>
	handler?.({ payload: { request_id: requestId, event } });

const drain = async (iterable: AsyncIterable<any>) => {
	const out: any[] = [];
	for await (const event of iterable) out.push(event);
	return out;
};

beforeEach(() => {
	mockInvoke.mockReset();
	handler = null;
});

describe('createLlmChat cancel', () => {
	it('re-issues llm_cancel once the request is registered when cancel arrives during setup', async () => {
		let finishSetup!: () => void;
		const cancels: string[] = [];
		mockInvoke.mockImplementation(async (command: string, args: any) => {
			if (command === 'llm_chat') {
				await new Promise<void>((resolve) => {
					finishSetup = resolve;
				});
			} else if (command === 'llm_cancel') {
				cancels.push(args.requestId);
			}
		});
		const { chat, cancel } = createLlmChat([], 1);
		const running = drain(chat([])).catch((e: Error) => e);
		await new Promise((resolve) => setTimeout(resolve, 0));
		await cancel();
		expect(cancels).toHaveLength(1);
		const requestId = cancels[0];
		finishSetup();
		await new Promise((resolve) => setTimeout(resolve, 0));
		expect(cancels).toEqual([requestId, requestId]);
		emit(requestId, { type: 'error', message: 'cancelled' });
		const result = await running;
		expect(result).toBeInstanceOf(Error);
	});

	it('throws on the next chat() after cancel, without starting a request', async () => {
		mockInvoke.mockResolvedValue(undefined);
		const { chat, cancel } = createLlmChat([], 1);
		await cancel();
		await expect(drain(chat([]))).rejects.toThrow('cancelled');
		expect(mockInvoke).not.toHaveBeenCalledWith('llm_chat', expect.anything());
	});
});

describe('createLlmChat tool calls', () => {
	it('assembles several calls by index, with arguments split across chunks', async () => {
		mockInvoke.mockImplementation(async (command: string, args: any) => {
			if (command !== 'llm_chat') return;
			const call = (index: number, id: any, name: any, argumentsPart: string) =>
				emit(args.requestId, {
					type: 'tool_call',
					index,
					id,
					name,
					arguments: argumentsPart,
				});
			emit(args.requestId, { type: 'delta', text: 'Looking.' });
			call(0, 'c0', 'tmgr_request', '{"method":"GET",');
			call(1, 'c1', 'tmgr_request', '{"method":"GET","path":"tasks/2"}');
			call(0, null, null, '"path":"tasks/1"}');
			emit(args.requestId, { type: 'done' });
		});
		const { chat } = createLlmChat([], 1);
		const events = await drain(chat([]));
		expect(events.map((e) => e.type)).toEqual([
			'text',
			'tool_call',
			'tool_call',
		]);
		expect(events[1].call).toMatchObject({
			id: 'c0',
			args: { method: 'GET', path: 'tasks/1' },
		});
		expect(events[2].call).toMatchObject({
			id: 'c1',
			args: { method: 'GET', path: 'tasks/2' },
		});
	});

	const run = async (
		chunks: { index?: number; id?: string | null; args: string }[],
	) => {
		mockInvoke.mockImplementation(async (command: string, args: any) => {
			if (command !== 'llm_chat') return;
			for (const chunk of chunks)
				emit(args.requestId, {
					type: 'tool_call',
					index: chunk.index,
					id: chunk.id,
					name: chunk.id ? 'tmgr_request' : null,
					arguments: chunk.args,
				});
			emit(args.requestId, { type: 'done' });
		});
		const { chat } = createLlmChat([], 1);
		return drain(chat([]));
	};
	const getCall = (path: string) => ({
		method: 'GET',
		path,
	});

	it('keeps two calls with different ids apart when they share an index', async () => {
		const events = await run([
			{ index: 0, id: 'a', args: '{"method":"GET","path":"tasks/1"}' },
			{ index: 0, id: 'b', args: '{"method":"GET","path":"tasks/2"}' },
		]);
		expect(events.map((e) => e.call.id)).toEqual(['a', 'b']);
		expect(events[0].call.args).toEqual(getCall('tasks/1'));
		expect(events[1].call.args).toEqual(getCall('tasks/2'));
	});

	it('keeps two calls with different ids apart when the index is missing', async () => {
		const events = await run([
			{ id: 'a', args: '{"method":"GET","path":"tasks/1"}' },
			{ id: 'b', args: '{"method":"GET","path":"tasks/2"}' },
		]);
		expect(events.map((e) => e.call.id)).toEqual(['a', 'b']);
		expect(events[1].call.args).toEqual(getCall('tasks/2'));
	});

	it('appends continuation chunks after the second id to the second call', async () => {
		const events = await run([
			{ index: 0, id: 'a', args: '{"method":"GET","path":"tasks/1"}' },
			{ index: 0, id: 'b', args: '{"method":"GET",' },
			{ index: 0, id: null, args: '"path":"tas' },
			{ index: 0, id: 'b', args: 'ks/2"}' },
		]);
		expect(events.map((e) => e.call.id)).toEqual(['a', 'b']);
		expect(events[0].call.args).toEqual(getCall('tasks/1'));
		expect(events[1].call.args).toEqual(getCall('tasks/2'));
	});

	it('still handles interleaved deltas with distinct indexes', async () => {
		const events = await run([
			{ index: 0, id: 'a', args: '{"method":"GET",' },
			{ index: 1, id: 'b', args: '{"method":"GET",' },
			{ index: 0, id: null, args: '"path":"tasks/1"}' },
			{ index: 1, id: null, args: '"path":"tasks/2"}' },
		]);
		expect(events.map((e) => e.call.id)).toEqual(['a', 'b']);
		expect(events[0].call.args).toEqual(getCall('tasks/1'));
		expect(events[1].call.args).toEqual(getCall('tasks/2'));
	});
});
