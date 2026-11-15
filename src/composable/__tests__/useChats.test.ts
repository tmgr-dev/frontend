jest.mock('@/actions/tmgr/chats', () => ({}));
jest.mock('@/composable/usePusher', () => ({ usePusher: jest.fn() }));

import type { Chat, ChatMessage } from '@/actions/tmgr/chats';
import {
	applyChatEvent,
	canDeleteMessage,
	canEditMessage,
	chatTitle,
	formatUnread,
	isClosedChannel,
	isMuted,
	markMessageDeleted,
	mergeMessages,
	totalUnread,
} from '@/composable/useChats';
import { findMentionQuery, insertMention } from '@/utils/chatMentions';

const msg = (id: number, over: Partial<ChatMessage> = {}): ChatMessage => ({
	id,
	chat_id: 1,
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
});

const chat = (id: number, over: Partial<Chat> = {}): Chat => ({
	id,
	workspace_id: 1,
	kind: 'channel',
	visibility: 'workspace',
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
});

describe('mergeMessages', () => {
	it('sorts ascending and dedupes by id', () => {
		const out = mergeMessages([msg(3), msg(1)], [msg(2), msg(3)]);
		expect(out.map((m) => m.id)).toEqual([1, 2, 3]);
	});

	it('replaces an existing message on update', () => {
		const out = mergeMessages([msg(1)], [msg(1, { body: 'new' })]);
		expect(out).toHaveLength(1);
		expect(out[0].body).toBe('new');
	});

	it('marks a message deleted', () => {
		const out = markMessageDeleted([msg(1), msg(2)], 2);
		expect(out[1].deleted).toBe(true);
		expect(out[1].body).toBeNull();
		expect(out[0].deleted).toBe(false);
	});
});

describe('unread', () => {
	const ctx = { openChatId: 2, selfId: 10 };
	const created = {
		chat_id: 1,
		message_id: 5,
		author_id: 11,
		preview: 'hi',
	};

	it('increments for a foreign message in a closed chat', () => {
		const { chats } = applyChatEvent(
			[chat(1), chat(2)],
			'chat.message.created',
			created,
			ctx,
		);
		expect(chats[0].unread).toBe(1);
		expect(chats[0].last_message?.preview).toBe('hi');
		expect(chats[1].unread).toBe(0);
	});

	it('does not increment for the open chat or own messages', () => {
		const open = applyChatEvent([chat(1)], 'chat.message.created', created, {
			...ctx,
			openChatId: 1,
		});
		expect(open.chats[0].unread).toBe(0);
		const own = applyChatEvent(
			[chat(1)],
			'chat.message.created',
			{ ...created, author_id: 10 },
			ctx,
		);
		expect(own.chats[0].unread).toBe(0);
	});

	it('ignores a repeated created event', () => {
		const once = applyChatEvent(
			[chat(1)],
			'chat.message.created',
			created,
			ctx,
		);
		const twice = applyChatEvent(
			once.chats,
			'chat.message.created',
			created,
			ctx,
		);
		expect(twice.chats[0].unread).toBe(1);
	});

	it('requests a reload for an unknown chat', () => {
		expect(
			applyChatEvent([], 'chat.message.created', created, ctx).reload,
		).toBe(true);
	});

	it('resets on read and removes a deleted chat', () => {
		const read = applyChatEvent(
			[chat(1, { unread: 4 })],
			'chat.read',
			{ chat_id: 1, last_read_message_id: 9 },
			ctx,
		);
		expect(read.chats[0].unread).toBe(0);
		const gone = applyChatEvent(
			[chat(1)],
			'chat.changed',
			{ chat_id: 1, action: 'deleted' },
			ctx,
		);
		expect(gone.chats).toEqual([]);
	});

	it('totals and caps the display', () => {
		expect(
			totalUnread([chat(1, { unread: 3 }), chat(2, { unread: null })]),
		).toBe(3);
		expect(formatUnread(0)).toBe('');
		expect(formatUnread(7)).toBe('7');
		expect(formatUnread(100)).toBe('99+');
	});
});

describe('chat helpers', () => {
	it('titles a DM by the other members', () => {
		const dm = chat(1, {
			kind: 'dm',
			title: null,
			members: [
				{ user_id: 10, name: 'Me', role: 'member', joined_at: null },
				{ user_id: 11, name: 'Bob', role: 'member', joined_at: null },
			],
		});
		expect(chatTitle(dm, 10)).toBe('Bob');
	});

	it('detects muted chats', () => {
		const now = Date.parse('2026-01-01T00:00:00Z');
		expect(isMuted(chat(1, { muted_until: '2026-01-02T00:00:00Z' }), now)).toBe(
			true,
		);
		expect(isMuted(chat(1, { muted_until: '2025-12-31T00:00:00Z' }), now)).toBe(
			false,
		);
		expect(isMuted(chat(1, { notify_level: 'none' }), now)).toBe(true);
	});
});

describe('mentions', () => {
	it('finds the query before the caret', () => {
		expect(findMentionQuery('hi @bo', 6)).toEqual({ start: 3, query: 'bo' });
		expect(findMentionQuery('mail a@b', 8)).toBeNull();
	});

	it('inserts the name', () => {
		const q = { start: 3, query: 'bo' };
		expect(insertMention('hi @bo', 6, q, 'Bob')).toEqual({
			text: 'hi @Bob ',
			caret: 8,
		});
	});
});

describe('message permissions', () => {
	const own = msg(1, { author: { kind: 'user', id: 10, name: 'Me' } });
	const other = msg(2, { author: { kind: 'user', id: 11, name: 'B' } });
	const persona = msg(3, { author: { kind: 'persona', id: 10, name: 'P' } });

	it('lets only the author edit', () => {
		expect(canEditMessage(own, 10)).toBe(true);
		expect(canEditMessage(other, 10)).toBe(false);
		expect(canEditMessage(persona, 10)).toBe(false);
		expect(canEditMessage(msg(4, { ...own, deleted: true }), 10)).toBe(false);
	});

	it('lets author, moderators and persona messages be deleted', () => {
		expect(canDeleteMessage(own, 10, false)).toBe(true);
		expect(canDeleteMessage(other, 10, false)).toBe(false);
		expect(canDeleteMessage(other, 10, true)).toBe(true);
		expect(canDeleteMessage(persona, 10, false)).toBe(true);
		expect(canDeleteMessage(msg(5, { ...own, deleted: true }), 10, true)).toBe(
			false,
		);
	});

	it('detects closed channels', () => {
		const closed = chat(1, { joined: false, can_read: false });
		expect(isClosedChannel(closed)).toBe(true);
		expect(isClosedChannel(chat(2))).toBe(false);
		expect(isClosedChannel(chat(3, { joined: false }))).toBe(false);
		expect(
			isClosedChannel(
				chat(4, {
					kind: null as unknown as Chat['kind'],
					joined: false,
					can_read: false,
				}),
			),
		).toBe(true);
	});
});
