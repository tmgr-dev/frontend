jest.mock('@/actions/tmgr/chats', () => ({
	getChat: jest.fn(),
	getMessages: jest.fn(),
	getMessage: jest.fn(),
	cancelPersonaReply: jest.fn(),
	getReplies: jest.fn(),
	sendMessage: jest.fn(),
	editMessage: jest.fn(),
	deleteMessage: jest.fn(),
	listChats: jest.fn(),
	markRead: jest.fn(),
}));
jest.mock('@/composable/usePusher', () => ({ usePusher: jest.fn() }));

import type { Chat, ChatMessage, ChatMessageHit } from '@/actions/tmgr/chats';
import * as api from '@/actions/tmgr/chats';
import { applyChatEvent, readableHits, useChats } from '@/composable/useChats';

const mocked = api as jest.Mocked<typeof api>;

const msg = (id: number, chatId: number, over: Partial<ChatMessage> = {}) =>
	({
		id,
		chat_id: chatId,
		author: { kind: 'user', id: 2, name: 'A' },
		body: `m${id}`,
		mentions: [],
		parent_message_id: null,
		reply_count: 0,
		last_reply_at: null,
		reactions: [],
		files: [],
		edited_at: null,
		deleted: false,
		created_at: '2026-01-01T00:00:00Z',
		...over,
	}) as ChatMessage;

const chat = (id: number, over: Partial<Chat> = {}) =>
	({
		id,
		workspace_id: 1,
		kind: 'channel',
		visibility: 'members',
		title: `c${id}`,
		topic: null,
		joined: true,
		can_read: true,
		can_write: true,
		can_moderate: false,
		can_delete: false,
		archived_at: null,
		last_message_at: null,
		last_message: null,
		unread: 0,
		member_count: 2,
		members: null,
		created_at: null,
		notify_level: null,
		muted_until: null,
		...over,
	}) as Chat;

const hit = (messageId: number, chatId: number) =>
	({
		message_id: messageId,
		chat_id: chatId,
		chat_title: `c${chatId}`,
		chat_kind: 'channel',
		parent_message_id: null,
		author: { kind: 'user', id: 2, name: 'A' },
		snippet: 'secret plan',
		created_at: '2026-01-01T00:00:00Z',
	}) as ChatMessageHit;

const deferred = <T>() => {
	let resolve!: (v: T) => void;
	const promise = new Promise<T>((r) => (resolve = r));
	return { promise, resolve };
};

describe('readableHits', () => {
	it('keeps hits only from chats the user can still read', () => {
		const hits = [hit(1, 1), hit(2, 2), hit(3, 3)];
		const chats = [chat(1), chat(2, { can_read: false, joined: false })];
		expect(readableHits(hits, chats).map((h) => h.message_id)).toEqual([1]);
	});

	it('drops a hit once its chat leaves the list', () => {
		const hits = [hit(1, 1)];
		expect(readableHits(hits, [chat(1)])).toHaveLength(1);
		expect(readableHits(hits, [])).toHaveLength(0);
	});
});

describe('chat switches', () => {
	const chats = useChats();

	beforeEach(() => {
		jest.resetAllMocks();
		chats.chats.value = [chat(1), chat(2)];
		mocked.getMessages.mockImplementation(async (id: number) => [
			msg(id * 100, id),
		]);
	});

	it('sends to the chat the composer belongs to and keeps it out of the open one', async () => {
		await chats.openChat(1);
		const pending = deferred<ChatMessage>();
		mocked.sendMessage.mockReturnValue(pending.promise);
		const sending = chats.send(1, { body: 'draft for A' });
		await chats.openChat(2);
		pending.resolve(msg(150, 1));
		await sending;
		expect(mocked.sendMessage).toHaveBeenCalledWith(1, { body: 'draft for A' });
		expect(chats.messages.value.map((m) => m.id)).toEqual([200]);
	});

	it('ignores an edit response from a chat that is no longer open', async () => {
		await chats.openChat(1);
		const pending = deferred<ChatMessage>();
		mocked.editMessage.mockReturnValue(pending.promise);
		const saving = chats.saveEdit(msg(100, 1), 'edited', []);
		await chats.openChat(2);
		pending.resolve(msg(100, 1, { body: 'edited' }));
		await saving;
		expect(chats.messages.value.map((m) => m.chat_id)).toEqual([2]);
	});

	it('ignores a delete response from a chat that is no longer open', async () => {
		await chats.openChat(1);
		const pending = deferred<void>();
		mocked.deleteMessage.mockReturnValue(pending.promise);
		const removing = chats.removeMessage(msg(100, 1));
		await chats.openChat(2);
		chats.messages.value = [msg(100, 2)];
		pending.resolve();
		await removing;
		expect(chats.messages.value[0].deleted).toBe(false);
	});
});

describe('unread from thread replies', () => {
	it('does not count a reply, matching the server count of top-level messages', () => {
		const result = applyChatEvent(
			[
				chat(1, {
					last_message: { id: 5, author_id: 2, preview: 'x', deleted: false },
				}),
			],
			'chat.message.created',
			{ chat_id: 1, message_id: 9, parent_message_id: 5, author_id: 3 },
			{ openChatId: null, selfId: 2 },
		);
		expect(result.chats[0].unread).toBe(0);
		expect(result.chats[0].last_message?.id).toBe(5);
	});
});

describe('persona replies queued by a send', () => {
	const chats = useChats();

	beforeEach(() => {
		jest.resetAllMocks();
		chats.chats.value = [chat(1)];
		mocked.getMessages.mockResolvedValue([msg(100, 1)]);
	});

	it('shows the pending persona message without waiting for realtime', async () => {
		await chats.openChat(1);
		mocked.sendMessage.mockResolvedValue(
			msg(101, 1, {
				persona_replies: [
					{ persona_id: 3, status: 'queued', message_id: 102 },
					{ persona_id: 4, status: 'refused', reason: 'busy' },
				],
			}),
		);
		mocked.getMessage.mockResolvedValue(
			msg(102, 1, {
				author: { kind: 'persona', id: 3, name: 'Scout' },
				status: 'pending',
			}),
		);
		await chats.send(1, { body: '@Scout hi' });
		expect(mocked.getMessage).toHaveBeenCalledTimes(1);
		expect(mocked.getMessage).toHaveBeenCalledWith(1, 102);
		expect(chats.messages.value.map((m) => [m.id, m.status])).toEqual([
			[100, undefined],
			[101, undefined],
			[102, 'pending'],
		]);
	});

	it('keeps a reply that realtime already brought in', async () => {
		await chats.openChat(1);
		mocked.sendMessage.mockResolvedValue(
			msg(101, 1, {
				persona_replies: [{ persona_id: 3, status: 'queued', message_id: 102 }],
			}),
		);
		let resolve!: (m: ChatMessage) => void;
		mocked.getMessage.mockReturnValue(new Promise((r) => (resolve = r)));
		const sending = chats.send(1, { body: '@Scout hi' });
		await Promise.resolve();
		await Promise.resolve();
		chats.messages.value = [
			...chats.messages.value,
			msg(102, 1, { status: 'done', body: 'answer' }),
		];
		resolve(msg(102, 1, { status: 'pending' }));
		await sending;
		expect(chats.messages.value.find((m) => m.id === 102)?.status).toBe('done');
	});
});

describe('stopping a persona reply', () => {
	const chats = useChats();
	const pending = () =>
		msg(102, 1, {
			author: { kind: 'persona', id: 3, name: 'Scout' },
			status: 'pending',
		});

	beforeEach(async () => {
		jest.resetAllMocks();
		chats.chats.value = [chat(1)];
		mocked.getMessages.mockResolvedValue([pending()]);
		await chats.openChat(1);
	});

	it('shows the real state when the reply already ended', async () => {
		mocked.cancelPersonaReply.mockRejectedValue(new Error('422'));
		mocked.getMessage.mockResolvedValue(msg(102, 1, { status: 'failed' }));
		await chats.stopReply(pending());
		expect(chats.messages.value[0].status).toBe('failed');
	});

	it('still fails when the reply is running', async () => {
		const error = new Error('500');
		mocked.cancelPersonaReply.mockRejectedValue(error);
		mocked.getMessage.mockResolvedValue(pending());
		await expect(chats.stopReply(pending())).rejects.toBe(error);
		expect(chats.messages.value[0].status).toBe('pending');
	});
});
