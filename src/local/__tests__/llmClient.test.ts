import {
	createLlmChat,
	llmComplete,
	llmDetect,
	llmLanConfirm,
	llmModels,
	llmTest,
	needsConfirmationOrigin,
} from '../llmClient';

type Handler = (e: { payload: { request_id: string; event: any } }) => void;

let handler: Handler | null = null;
let mockListenOptions: unknown = null;
const mockInvoke = jest.fn();

jest.mock('@tauri-apps/api/core', () => ({
	invoke: (...args: unknown[]) => mockInvoke(...args),
}));
jest.mock('@tauri-apps/api/event', () => ({
	listen: async (_name: string, fn: Handler, options?: unknown) => {
		handler = fn;
		mockListenOptions = options;
		return () => {
			handler = null;
		};
	},
}));
jest.mock('@tauri-apps/api/webviewWindow', () => ({
	getCurrentWebviewWindow: () => ({ label: 'task-42' }),
}));

const order: string[] = [];
let mockResync: () => Promise<void> = async () => {
	order.push('resync');
};
jest.mock('@/utils/cloudIdentity', () => ({
	resyncCloudIdentity: () => mockResync(),
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
	mockListenOptions = null;
	mockResync = async () => {
		order.push('resync');
	};
});

describe('createLlmChat event routing', () => {
	it('listens only to events sent to the current window', async () => {
		mockInvoke.mockImplementation(async (command: string, args: any) => {
			if (command === 'llm_chat') emit(args.requestId, { type: 'done' });
		});
		const { chat } = createLlmChat([], 1);
		await drain(chat([]));
		expect(mockListenOptions).toEqual({ target: 'task-42' });
	});
});

describe('createLlmChat limits', () => {
	it('sends the token cap and the time left until the deadline', async () => {
		mockInvoke.mockImplementation(async (command: string, args: any) => {
			if (command === 'llm_chat') emit(args.requestId, { type: 'done' });
		});
		const { chat } = createLlmChat([], 1, {
			maxTokens: 8000,
			deadlineAt: Date.now() + 60_000,
		});
		await drain(chat([]));
		const args = mockInvoke.mock.calls.find((c) => c[0] === 'llm_chat')![1];
		expect(args.maxTokens).toBe(8000);
		expect(args.totalTimeoutMs).toBeGreaterThan(50_000);
		expect(args.totalTimeoutMs).toBeLessThanOrEqual(60_000);
	});

	it('stops with a clear message once the overall deadline has passed', async () => {
		mockInvoke.mockResolvedValue(undefined);
		const { chat } = createLlmChat([], 1, { deadlineAt: Date.now() - 1 });
		await expect(drain(chat([]))).rejects.toThrow(
			'The answer took longer than 15 minutes and was stopped.',
		);
		expect(mockInvoke).not.toHaveBeenCalledWith('llm_chat', expect.anything());
	});
});

describe('createLlmChat cancel', () => {
	it('does not send the prompt when cancelled during identity resync', async () => {
		let releaseResync!: () => void;
		mockResync = () =>
			new Promise<void>((resolve) => {
				releaseResync = resolve;
			});
		mockInvoke.mockImplementation(async (command: string, args: any) => {
			if (command === 'llm_chat') emit(args.requestId, { type: 'done' });
		});
		const { chat, cancel } = createLlmChat([], 1);
		const running = drain(chat([])).catch((e: Error) => e);
		await new Promise((resolve) => setTimeout(resolve, 0));
		await cancel();
		releaseResync();
		const result = await running;
		expect(result).toBeInstanceOf(Error);
		expect((result as Error).message).toBe('cancelled');
		expect(mockInvoke).not.toHaveBeenCalledWith('llm_chat', expect.anything());
		expect(handler).toBeNull();
	});

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

describe('createLlmChat missing tool call fields', () => {
	const runWithoutIds = async (
		chunks: { index: number; args: string }[],
	): Promise<{ events: any[]; requestId: string }> => {
		let requestId = '';
		mockInvoke.mockImplementation(async (command: string, args: any) => {
			if (command !== 'llm_chat') return;
			requestId = args.requestId;
			for (const chunk of chunks)
				emit(args.requestId, {
					type: 'tool_call',
					index: chunk.index,
					id: null,
					name: 'tmgr_request',
					arguments: chunk.args,
				});
			emit(args.requestId, { type: 'done' });
		});
		const { chat } = createLlmChat([], 1);
		return { events: await drain(chat([])), requestId };
	};

	it('assigns each call without an id a unique fallback id tied to the request and position', async () => {
		const { events, requestId } = await runWithoutIds([
			{ index: 0, args: '{"method":"GET","path":"tasks/1"}' },
			{ index: 1, args: '{"method":"GET","path":"tasks/2"}' },
		]);
		expect(events.map((e) => e.call.id)).toEqual([
			`call_${requestId}_0`,
			`call_${requestId}_1`,
		]);
	});

	it('echoes {} as the raw arguments when a call sends none', async () => {
		const { events } = await runWithoutIds([{ index: 0, args: '' }]);
		expect(events[0].call.rawArguments).toBe('{}');
		expect(events[0].call.args).toEqual({});
	});

	it('truncates invalid tool arguments in the error to 200 characters', async () => {
		mockInvoke.mockImplementation(async (command: string, args: any) => {
			if (command !== 'llm_chat') return;
			emit(args.requestId, {
				type: 'tool_call',
				index: 0,
				id: 'c0',
				name: 'tmgr_request',
				arguments: 'x'.repeat(500),
			});
			emit(args.requestId, { type: 'done' });
		});
		const { chat } = createLlmChat([], 1);
		const error = await drain(chat([])).catch((e: Error) => e);
		expect((error as Error).message).toBe(
			`The model sent invalid tool arguments: ${'x'.repeat(200)}…`,
		);
	});

	it('keeps short invalid tool arguments whole in the error', async () => {
		mockInvoke.mockImplementation(async (command: string, args: any) => {
			if (command !== 'llm_chat') return;
			emit(args.requestId, {
				type: 'tool_call',
				index: 0,
				id: 'c0',
				name: 'tmgr_request',
				arguments: '{"method":',
			});
			emit(args.requestId, { type: 'done' });
		});
		const { chat } = createLlmChat([], 1);
		const error = await drain(chat([])).catch((e: Error) => e);
		expect((error as Error).message).toBe(
			'The model sent invalid tool arguments: {"method":',
		);
	});
});

describe('createLlmChat identity resync', () => {
	it('resyncs the shared identity before llm_chat', async () => {
		order.length = 0;
		let requestId = '';
		mockInvoke.mockImplementation(async (command: string, args: any) => {
			order.push(command);
			if (command === 'llm_chat') requestId = args.requestId;
		});
		const { chat } = createLlmChat([], 1);
		const running = drain(chat([]));
		await new Promise((resolve) => setTimeout(resolve, 0));
		emit(requestId, { type: 'done' });
		await running;
		expect(order).toEqual(['resync', 'llm_chat']);
	});
});

describe('llm command wrappers', () => {
	const answer = (value: unknown) =>
		mockInvoke.mockImplementation(async (command: string) => {
			order.push(command);
			return value;
		});

	beforeEach(() => {
		order.length = 0;
	});

	it('llmDetect invokes llm_detect without resyncing identity', async () => {
		const found = [{ base_url: 'http://127.0.0.1:1234', models: 2 }];
		answer(found);
		await expect(llmDetect()).resolves.toEqual(found);
		expect(order).toEqual(['llm_detect']);
		expect(mockInvoke.mock.calls[0][0]).toBe('llm_detect');
	});

	it('llmModels resyncs identity, then sends the user id', async () => {
		answer(['qwen3', 'llama']);
		await expect(llmModels(7)).resolves.toEqual(['qwen3', 'llama']);
		expect(order).toEqual(['resync', 'llm_models']);
		expect(mockInvoke).toHaveBeenCalledWith('llm_models', { userId: 7 });
	});

	it('llmTest resyncs identity, then returns the verdict', async () => {
		answer('ok: qwen3');
		await expect(llmTest(7)).resolves.toBe('ok: qwen3');
		expect(order).toEqual(['resync', 'llm_test']);
		expect(mockInvoke).toHaveBeenCalledWith('llm_test', { userId: 7 });
	});

	it('llmComplete resyncs identity, then sends the prompt options', async () => {
		answer('summary');
		const result = await llmComplete(7, {
			system: 'be brief',
			prompt: 'day input',
			maxTokens: 300,
			temperature: 0.2,
		});
		expect(result).toBe('summary');
		expect(order).toEqual(['resync', 'llm_complete']);
		expect(mockInvoke).toHaveBeenCalledWith('llm_complete', {
			userId: 7,
			system: 'be brief',
			prompt: 'day input',
			maxTokens: 300,
			temperature: 0.2,
		});
	});

	it('llmLanConfirm resyncs identity then invokes llm_lan_confirm with the user', async () => {
		answer(undefined);
		await llmLanConfirm(7, 'http://192.0.2.5:1234', true);
		expect(order).toEqual(['resync', 'llm_lan_confirm']);
		expect(mockInvoke).toHaveBeenCalledWith('llm_lan_confirm', {
			origin: 'http://192.0.2.5:1234',
			confirmed: true,
			userId: 7,
		});
	});

	it('reads the origin out of a rejected llmComplete', async () => {
		mockInvoke.mockRejectedValue('needs_confirmation:http://192.0.2.5:1234');
		const error = await llmComplete(7, {
			system: '',
			prompt: '',
			maxTokens: 1,
			temperature: 0,
		}).catch((e: unknown) => e);
		expect(needsConfirmationOrigin(error)).toBe('http://192.0.2.5:1234');
	});
});

describe('needsConfirmationOrigin', () => {
	it('parses the origin from an Error', () => {
		expect(
			needsConfirmationOrigin(
				new Error('needs_confirmation:http://192.0.2.5:1234'),
			),
		).toBe('http://192.0.2.5:1234');
	});

	it('parses the origin from a plain string', () => {
		expect(
			needsConfirmationOrigin('needs_confirmation:http://192.0.2.2:8080'),
		).toBe('http://192.0.2.2:8080');
	});

	it('returns null for other errors', () => {
		expect(needsConfirmationOrigin(new Error('connection refused'))).toBeNull();
		expect(needsConfirmationOrigin('llm request failed')).toBeNull();
		expect(needsConfirmationOrigin(undefined)).toBeNull();
		expect(
			needsConfirmationOrigin({ message: 'needs_confirmation:x' }),
		).toBeNull();
	});

	it('returns null when the origin is missing', () => {
		expect(needsConfirmationOrigin('needs_confirmation:')).toBeNull();
		expect(
			needsConfirmationOrigin(new Error('needs_confirmation:   ')),
		).toBeNull();
	});
});
