import { useAgentChat } from '@/composable/useAgentChat';
import type { AgentMessage } from '@/types/agent';

const mockGet = jest.fn();
const mockPost = jest.fn();
const mockSubscribeToUser = jest.fn((..._args: unknown[]) => 'sub-1');

jest.mock('@/plugins/axios', () => ({
	__esModule: true,
	default: {
		get: (...args: unknown[]) => mockGet(...args),
		post: (...args: unknown[]) => mockPost(...args),
	},
}));
jest.mock('@/store', () => ({ __esModule: true, default: { state: {} } }));
jest.mock('@/composable/usePusher', () => ({
	usePusher: () => ({
		subscribeToUser: mockSubscribeToUser,
		unsubscribeHandler: jest.fn(),
	}),
}));

const done = (id: number): AgentMessage => ({
	id,
	role: id % 2 ? 'user' : 'assistant',
	content: `message ${id}`,
	status: 'done',
	steps: [],
	created_at: '',
});

const ready = (messages = [done(1), done(2)]) => {
	const chat = useAgentChat();
	chat.state.value = {
		conversationId: 5,
		workspaceId: 1,
		messages,
		pendingId: null,
	};
	return chat;
};

describe('compaction history', () => {
	beforeEach(() => {
		mockGet.mockReset();
		mockPost.mockReset();
		mockSubscribeToUser.mockClear();
	});

	it('loads compactions with the messages and tolerates older servers', async () => {
		mockGet.mockResolvedValueOnce({
			data: {
				data: [done(1), done(2)],
				compactions: [
					{
						id: 7,
						up_to_message_id: 2,
						trigger: 'manual',
						created_at: '',
					},
				],
			},
		});
		const chat = useAgentChat();
		chat.state.value = { ...chat.state.value, workspaceId: 1 };

		await chat.openConversation(5);
		expect(chat.compactions.value.map((c) => c.id)).toEqual([7]);

		mockGet.mockResolvedValueOnce({ data: { data: [done(1)] } });
		await chat.openConversation(5);
		expect(chat.compactions.value).toEqual([]);
	});

	it('is available only when there are two finished messages and no pending one', () => {
		expect(ready().canCompact.value).toBe(true);
		expect(ready([done(1)]).canCompact.value).toBe(false);
		expect(
			ready([done(1), done(2), { ...done(3), status: 'pending' }]).canCompact
				.value,
		).toBe(false);
	});

	it('adds the marker from the POST response', async () => {
		mockPost.mockResolvedValue({
			data: {
				data: {
					id: 9,
					up_to_message_id: 2,
					trigger: 'manual',
					created_at: '',
				},
			},
		});
		const chat = ready();

		await chat.compact();

		expect(mockPost).toHaveBeenCalledWith('/agent/conversations/5/compact');
		expect(chat.compactions.value.map((c) => c.id)).toEqual([9]);
		expect(chat.compacting.value).toBe(false);
		expect(chat.error.value).toBeNull();
	});

	it('surfaces the server message for a 409', async () => {
		mockPost.mockRejectedValue({
			response: { status: 409, data: { message: 'A turn is in flight.' } },
		});
		const chat = ready();

		await chat.compact();

		expect(chat.error.value).toBe('A turn is in flight.');
		expect(chat.compactions.value).toEqual([]);
		expect(chat.compacting.value).toBe(false);
	});

	it('merges agent.compacted only for the open conversation', () => {
		const chat = ready();
		chat.subscribe(42);
		const handlers = mockSubscribeToUser.mock.calls[0][1] as {
			onAgentCompacted: (e: unknown) => void;
		};

		handlers.onAgentCompacted({
			conversation_id: 99,
			compaction_id: 3,
			up_to_message_id: 2,
			trigger: 'auto',
		});
		expect(chat.compactions.value).toEqual([]);

		handlers.onAgentCompacted({
			conversation_id: 5,
			compaction_id: 3,
			up_to_message_id: 2,
			trigger: 'auto',
		});
		handlers.onAgentCompacted({
			conversation_id: 5,
			compaction_id: 3,
			up_to_message_id: 2,
			trigger: 'auto',
		});
		expect(chat.compactions.value.map((c) => [c.id, c.trigger])).toEqual([
			[3, 'auto'],
		]);
	});
});
