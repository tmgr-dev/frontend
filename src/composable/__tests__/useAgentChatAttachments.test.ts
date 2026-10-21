import { attachConversationFile, sendAgentMessage } from '@/actions/tmgr/agent';
import { useAgentChat } from '@/composable/useAgentChat';

const mockPost = jest.fn();

jest.mock('@/plugins/axios', () => ({
	__esModule: true,
	default: { post: (...args: unknown[]) => mockPost(...args) },
}));
jest.mock('@/store', () => ({ __esModule: true, default: { state: {} } }));
jest.mock('@/composable/usePusher', () => ({ usePusher: () => ({}) }));

const target = {
	key: 'uploads/k.pdf',
	content_type: 'application/pdf',
	upload_url: 'https://s/u',
} as never;

describe('attachment actions', () => {
	beforeEach(() => mockPost.mockReset());

	it('sends file_ids only when present', async () => {
		mockPost.mockResolvedValue({ data: { data: { message_id: 1 } } });
		await sendAgentMessage(5, 'hi');
		await sendAgentMessage(5, 'hi', undefined, []);
		await sendAgentMessage(5, 'hi', undefined, [3, 4]);
		expect(mockPost.mock.calls[0][1]).toEqual({ content: 'hi' });
		expect(mockPost.mock.calls[1][1]).toEqual({ content: 'hi' });
		expect(mockPost.mock.calls[2][1]).toEqual({
			content: 'hi',
			file_ids: [3, 4],
		});
	});

	it('posts the presigned key to the conversation files endpoint', async () => {
		const saved = {
			id: 9,
			name: 'a.pdf',
			mime_type: 'application/pdf',
			size: 3,
		};
		mockPost.mockResolvedValue({ data: { data: saved } });
		const file = new File(['abc'], 'a.pdf', { type: 'application/pdf' });
		await expect(attachConversationFile(5, file, target)).resolves.toEqual(
			saved,
		);
		expect(mockPost).toHaveBeenCalledWith('/agent/conversations/5/files', {
			file_name: 'a.pdf',
			file_path: 'uploads/k.pdf',
			mime_type: 'application/pdf',
			size_bytes: 3,
		});
	});
});

describe('useAgentChat send with files', () => {
	beforeEach(() => mockPost.mockReset());

	const ready = () => {
		const chat = useAgentChat();
		chat.state.value = {
			...chat.state.value,
			conversationId: 5,
			workspaceId: 1,
		};
		return chat;
	};

	it('passes file ids and shows files on the optimistic message', async () => {
		mockPost.mockResolvedValue({
			data: { data: { message_id: 10, pending_message_id: 11 } },
		});
		const chat = ready();
		const files = [{ id: 3, name: 'a.pdf', mime_type: null, size: 3 }];
		expect(await chat.send('hi', undefined, [3], files)).toBe(true);
		expect(mockPost).toHaveBeenCalledWith('/agent/conversations/5/messages', {
			content: 'hi',
			file_ids: [3],
		});
		expect(chat.state.value.messages[0].files).toEqual(files);
	});

	it('reports failure with the mapped error and keeps state', async () => {
		mockPost.mockRejectedValue({
			response: { status: 422, data: { error: 'too_many_files' } },
		});
		const chat = ready();
		expect(await chat.send('hi', undefined, [3], [])).toBe(false);
		expect(chat.error.value).toMatch(/up to 5 files/);
		expect(chat.state.value.messages).toHaveLength(0);
	});

	it('ensureConversation returns the active conversation id', async () => {
		const chat = ready();
		await expect(chat.ensureConversation()).resolves.toBe(5);
	});
});
