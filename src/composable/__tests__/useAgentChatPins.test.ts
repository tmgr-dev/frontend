import { useAgentChat } from '@/composable/useAgentChat';
import type { AgentPin, AgentPinLimits } from '@/types/agent';

const mockGet = jest.fn();
const mockPost = jest.fn();
const mockPut = jest.fn();
const mockDelete = jest.fn();
const mockSubscribeToUser = jest.fn((..._args: unknown[]) => 'sub-1');

jest.mock('@/plugins/axios', () => ({
	__esModule: true,
	default: {
		get: (...args: unknown[]) => mockGet(...args),
		post: (...args: unknown[]) => mockPost(...args),
		put: (...args: unknown[]) => mockPut(...args),
		delete: (...args: unknown[]) => mockDelete(...args),
	},
}));
jest.mock('@/store', () => ({ __esModule: true, default: { state: {} } }));
jest.mock('@/composable/usePusher', () => ({
	usePusher: () => ({
		subscribeToUser: mockSubscribeToUser,
		unsubscribeHandler: jest.fn(),
	}),
}));

const limits: AgentPinLimits = {
	max_pins: 20,
	max_chars: 500,
	max_total_chars: 4000,
};

const pin = (id: number, content = `fact ${id}`): AgentPin => ({
	id,
	content,
	source_message_id: null,
	created_at: '',
	updated_at: '',
});

const flush = () => new Promise((resolve) => setTimeout(resolve, 0));

const deferred = <T>() => {
	let resolve!: (value: T) => void;
	const promise = new Promise<T>((r) => {
		resolve = r;
	});
	return { promise, resolve };
};

const ready = (pins: AgentPin[] = []) => {
	const chat = useAgentChat();
	chat.state.value = {
		conversationId: 5,
		workspaceId: 1,
		messages: [],
		pendingId: null,
	};
	chat.pins.value = pins;
	return chat;
};

describe('pinned facts', () => {
	beforeEach(() => {
		mockGet.mockReset();
		mockPost.mockReset();
		mockPut.mockReset();
		mockDelete.mockReset();
		mockSubscribeToUser.mockClear();
	});

	it('loads pins and limits from GET for the opened conversation', async () => {
		mockGet.mockImplementation(async (url: string) =>
			url.endsWith('/pins')
				? { data: { data: [pin(1)], limits: { ...limits, max_pins: 7 } } }
				: { data: { data: [] } },
		);
		const chat = useAgentChat();
		chat.state.value = { ...chat.state.value, workspaceId: 1 };

		await chat.openConversation(5);
		await flush();

		expect(chat.pins.value.map((p) => p.id)).toEqual([1]);
		expect(chat.pinLimits.value.max_pins).toBe(7);
		expect(chat.pinsLoading.value).toBe(false);
	});

	it('ignores pins loaded for a chat the user has already left', async () => {
		const slow = deferred<unknown>();
		mockGet.mockImplementation((url: string) => {
			if (url.endsWith('/pins') && url.includes('/5/')) return slow.promise;
			if (url.endsWith('/pins'))
				return Promise.resolve({
					data: { data: [pin(9, 'chat six')], limits },
				});
			return Promise.resolve({ data: { data: [] } });
		});
		const chat = useAgentChat();
		chat.state.value = { ...chat.state.value, workspaceId: 1 };

		const first = chat.openConversation(5);
		await chat.openConversation(6);
		await flush();
		slow.resolve({ data: { data: [pin(1, 'chat five')], limits } });
		await first;
		await flush();

		expect(chat.state.value.conversationId).toBe(6);
		expect(chat.pins.value.map((p) => p.content)).toEqual(['chat six']);
	});

	it('creates the conversation before saving the first pin', async () => {
		mockPost.mockImplementation(async (url: string) =>
			url === '/agent/conversations'
				? {
						data: {
							data: {
								id: 5,
								workspace_id: 1,
								task_id: null,
								created_at: '',
							},
						},
					}
				: {
						data: {
							data: pin(1, 'Use EUR'),
						},
					},
		);
		mockGet.mockImplementation(async (url: string) =>
			url.endsWith('/pins')
				? { data: { data: [], limits } }
				: { data: { data: [] } },
		);
		const chat = useAgentChat();
		chat.state.value = { ...chat.state.value, workspaceId: 1 };

		const saved = await chat.addPin('Use EUR');

		expect(saved).toBe(true);
		expect(mockPost.mock.calls.map((c) => c[0])).toEqual([
			'/agent/conversations',
			'/agent/conversations/5/pins',
		]);
		expect(mockPost).toHaveBeenLastCalledWith('/agent/conversations/5/pins', {
			content: 'Use EUR',
		});
		expect(chat.pins.value.map((p) => p.content)).toEqual(['Use EUR']);
	});

	it('shows the server cap message when a pin is rejected', async () => {
		mockPost.mockRejectedValue({
			response: {
				status: 422,
				data: { message: 'You can keep at most 20 pinned facts.' },
			},
		});
		const chat = ready();

		const saved = await chat.addPin('one more');

		expect(saved).toBe(false);
		expect(chat.pinError.value).toBe('You can keep at most 20 pinned facts.');
		expect(chat.pins.value).toEqual([]);
		expect(chat.pinSaving.value).toBe(false);
	});

	it('does not send a pin that is over the local cap', async () => {
		const chat = ready(Array.from({ length: 20 }, (_, i) => pin(i + 1)));

		expect(await chat.addPin('one more')).toBe(false);
		expect(mockPost).not.toHaveBeenCalled();
	});

	it('adds with the source message, edits and removes a pin', async () => {
		mockPost.mockResolvedValue({
			data: { data: { ...pin(3, 'from reply'), source_message_id: 8 } },
		});
		mockPut.mockResolvedValue({ data: { data: pin(3, 'edited') } });
		mockDelete.mockResolvedValue({});
		const chat = ready([pin(1)]);

		await chat.addPin('from reply', 8);
		expect(mockPost).toHaveBeenCalledWith('/agent/conversations/5/pins', {
			content: 'from reply',
			source_message_id: 8,
		});
		expect(chat.pins.value.map((p) => p.id)).toEqual([1, 3]);

		expect(await chat.editPin(3, 'edited')).toBe(true);
		expect(mockPut).toHaveBeenCalledWith('/agent/conversations/5/pins/3', {
			content: 'edited',
		});
		expect(chat.pins.value.map((p) => p.content)).toEqual(['fact 1', 'edited']);

		expect(await chat.removePin(1)).toBe(true);
		expect(mockDelete).toHaveBeenCalledWith('/agent/conversations/5/pins/1');
		expect(chat.pins.value.map((p) => p.id)).toEqual([3]);
	});

	it('shows the generic message when a delete fails', async () => {
		mockDelete.mockRejectedValue({ response: { status: 500 } });
		const chat = ready([pin(1)]);

		expect(await chat.removePin(1)).toBe(false);
		expect(chat.pinError.value).toBe(
			'Could not save the pinned fact. Please try again.',
		);
		expect(chat.pins.value.map((p) => p.id)).toEqual([1]);
	});
});
