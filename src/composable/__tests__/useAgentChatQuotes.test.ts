import { useAgentChat } from '@/composable/useAgentChat';

const mockGet = jest.fn();
const mockPost = jest.fn();

jest.mock('@/plugins/axios', () => ({
	__esModule: true,
	default: {
		get: (...args: unknown[]) => mockGet(...args),
		post: (...args: unknown[]) => mockPost(...args),
	},
}));
jest.mock('@/store', () => ({ __esModule: true, default: { state: {} } }));
jest.mock('@/composable/usePusher', () => ({ usePusher: () => ({}) }));

const saved = {
	message_id: 4,
	role: 'assistant',
	created_at: '2026-10-20T10:00:00Z',
	text: 'bit',
	whole: false,
};

const ready = () => {
	const chat = useAgentChat();
	chat.state.value = { ...chat.state.value, conversationId: 5, workspaceId: 1 };
	return chat;
};

describe('composer quotes', () => {
	beforeEach(() => {
		mockGet.mockReset();
		mockPost.mockReset();
		mockGet.mockResolvedValue({ data: { data: [] } });
	});

	it('sends quotes, shows the saved ones and clears the composer', async () => {
		mockPost.mockResolvedValue({
			data: {
				data: { message_id: 10, pending_message_id: 11, quotes: [saved] },
			},
		});
		const chat = ready();
		chat.addQuoteDraft({ message_id: 4, text: 'bit' });
		chat.addQuoteDraft({ message_id: 3, text: null });

		expect(await chat.send('hi')).toBe(true);

		expect(mockPost).toHaveBeenCalledWith('/agent/conversations/5/messages', {
			content: 'hi',
			quotes: [
				{ message_id: 4, text: 'bit' },
				{ message_id: 3, text: null },
			],
		});
		expect(chat.state.value.messages[0].quotes).toEqual([saved]);
		expect(chat.quotes.value).toEqual([]);
	});

	it('omits quotes when there are none', async () => {
		mockPost.mockResolvedValue({
			data: { data: { message_id: 10, pending_message_id: 11 } },
		});
		const chat = ready();
		await chat.send('hi');
		expect(mockPost.mock.calls[0][1]).toEqual({ content: 'hi' });
		expect(chat.state.value.messages[0]).not.toHaveProperty('quotes');
	});

	it('keeps the quotes when sending fails', async () => {
		mockPost.mockRejectedValue({ response: { status: 500 } });
		const chat = ready();
		chat.addQuoteDraft({ message_id: 4, text: null });
		expect(await chat.send('hi')).toBe(false);
		expect(chat.quotes.value).toHaveLength(1);
	});

	it('does not send quotes with a scenario run', async () => {
		mockPost.mockResolvedValue({
			data: { data: { message_id: 10, pending_message_id: 11 } },
		});
		const chat = ready();
		chat.addQuoteDraft({ message_id: 4, text: null });
		await chat.send('Overdue', 'overdue');
		expect(mockPost.mock.calls[0][1]).not.toHaveProperty('quotes');
		expect(chat.quotes.value).toHaveLength(1);
	});

	it('dedupes, enforces the limit and reports it', () => {
		const chat = ready();
		expect(chat.addQuoteDraft({ message_id: 1, text: null })).toBe(true);
		expect(chat.addQuoteDraft({ message_id: 1, text: null })).toBe(false);
		expect(chat.error.value).toBeNull();
		for (let id = 2; id <= 5; id++)
			chat.addQuoteDraft({ message_id: id, text: null });
		expect(chat.addQuoteDraft({ message_id: 6, text: null })).toBe(false);
		expect(chat.quotes.value).toHaveLength(5);
		expect(chat.error.value).toMatch(/up to 5/);
	});

	it('removes one quote by index', () => {
		const chat = ready();
		chat.addQuoteDraft({ message_id: 1, text: null });
		chat.addQuoteDraft({ message_id: 2, text: null });
		chat.removeQuoteDraft(0);
		expect(chat.quotes.value).toEqual([{ message_id: 2, text: null }]);
	});

	it('clears the quotes when another conversation is opened', async () => {
		const chat = ready();
		chat.addQuoteDraft({ message_id: 1, text: null });
		await chat.openConversation(6);
		expect(chat.quotes.value).toEqual([]);
	});

	it('clears the quotes on a new chat', async () => {
		mockPost.mockResolvedValue({
			data: { data: { id: 9, workspace_id: 1, task_id: null, created_at: '' } },
		});
		const chat = ready();
		chat.addQuoteDraft({ message_id: 1, text: null });
		await chat.newChat();
		expect(chat.quotes.value).toEqual([]);
	});

	it('clears the quotes when the workspace changes', async () => {
		mockPost.mockResolvedValue({
			data: { data: { id: 9, workspace_id: 2, task_id: null, created_at: '' } },
		});
		const chat = ready();
		chat.addQuoteDraft({ message_id: 1, text: null });
		await chat.load(2);
		expect(chat.quotes.value).toEqual([]);
	});
});
