import type { Chat, ChatMessage, ForwardDto } from '@/actions/tmgr/chats';
import {
	canCreateTaskFromMessage,
	canForwardMessage,
	forwardCardView,
	forwardErrorKey,
	forwardSnapshotLabel,
	isOpenChannel,
	messageLink,
	taskDraftFromMessage,
	taskLink,
} from '@/utils/chatForward';

const chat = (over: Partial<Chat> = {}): Chat =>
	({ kind: 'channel', visibility: 'workspace', ...over }) as Chat;

const message = (over: Partial<ChatMessage> = {}): ChatMessage =>
	({
		id: 7,
		chat_id: 3,
		body: 'hi',
		deleted: false,
		author: { kind: 'user', id: 1, name: 'A' },
		...over,
	}) as ChatMessage;

const fwd = (over: Partial<ForwardDto> = {}): ForwardDto => ({
	type: 'task',
	id: 5,
	state: 'live',
	...over,
});

describe('isOpenChannel', () => {
	it('accepts only workspace-visible channels', () => {
		expect(isOpenChannel(chat())).toBe(true);
		expect(isOpenChannel(chat({ visibility: 'members' }))).toBe(false);
		expect(isOpenChannel(chat({ kind: 'dm' }))).toBe(false);
		expect(isOpenChannel(chat({ kind: 'group_dm' }))).toBe(false);
		expect(isOpenChannel(null)).toBe(false);
	});
});

describe('canForwardMessage / canCreateTaskFromMessage', () => {
	it('requires an open channel and a settled live message', () => {
		expect(canForwardMessage(message(), chat())).toBe(true);
		expect(canForwardMessage(message({ status: 'done' }), chat())).toBe(true);
		expect(canForwardMessage(message(), chat({ visibility: 'members' }))).toBe(
			false,
		);
		expect(canForwardMessage(message({ deleted: true }), chat())).toBe(false);
		expect(canForwardMessage(message({ status: 'pending' }), chat())).toBe(
			false,
		);
		expect(canForwardMessage(message({ status: 'failed' }), chat())).toBe(
			false,
		);
		expect(canForwardMessage(message({ status: 'canceled' }), chat())).toBe(
			false,
		);
		expect(canForwardMessage(message(), null)).toBe(false);
	});

	it('create-task follows the same rule', () => {
		expect(canCreateTaskFromMessage(message(), chat())).toBe(true);
		expect(canCreateTaskFromMessage(message(), chat({ kind: 'dm' }))).toBe(
			false,
		);
	});
});

describe('forwardSnapshotLabel', () => {
	it('prefers the key, then the title', () => {
		expect(forwardSnapshotLabel(fwd({ key: 'TM-123', title: 'T' }))).toBe(
			'TM-123',
		);
		expect(forwardSnapshotLabel(fwd({ key: null, title: 'general' }))).toBe(
			'general',
		);
		expect(forwardSnapshotLabel(fwd())).toBe('');
	});
});

describe('forwardCardView', () => {
	it('live shows content and no flags', () => {
		const v = forwardCardView(fwd({ text: 'x', file_names: ['a.txt'] }));
		expect(v).toMatchObject({
			deleted: false,
			edited: false,
			text: 'x',
			fileNames: ['a.txt'],
		});
	});
	it('edited flags the card', () => {
		expect(forwardCardView(fwd({ state: 'edited', text: 'x' })).edited).toBe(
			true,
		);
	});
	it('deleted drops text and files', () => {
		const v = forwardCardView(
			fwd({ state: 'deleted', text: 'secret', file_names: ['a'] }),
		);
		expect(v).toMatchObject({ deleted: true, text: '', fileNames: [] });
	});
});

describe('forwardErrorKey', () => {
	const err = (status: number, error?: string) => ({
		response: { status, data: { error } },
	});
	it('maps backend codes', () => {
		expect(forwardErrorKey(err(422, 'forward_not_allowed'))).toBe(
			'chats.forward.notAllowed',
		);
		expect(forwardErrorKey(err(404, 'forward_source_not_found'))).toBe(
			'chats.forward.sourceNotFound',
		);
	});
	it('falls back for anything else', () => {
		expect(forwardErrorKey(err(500))).toBe('chats.forward.failed');
		expect(forwardErrorKey(new Error('x'))).toBe('chats.forward.failed');
		expect(forwardErrorKey(null)).toBe('chats.forward.failed');
	});
});

describe('links', () => {
	it('builds task and message links', () => {
		expect(taskLink('ws', 9)).toBe('/ws/tasks/9');
		expect(messageLink('ws', 3, 7)).toBe('/ws/chats/3?message=7');
	});
});

describe('taskDraftFromMessage', () => {
	it('uses the first non-empty line as the title', () => {
		const d = taskDraftFromMessage('\n  \n  Fix login  \nmore', 'ws', 3, 7);
		expect(d.title).toBe('Fix login');
		expect(d.description).toBe(
			'\n  \n  Fix login  \nmore\n\n/ws/chats/3?message=7',
		);
	});
	it('caps the title at 255 characters', () => {
		expect(
			taskDraftFromMessage('x'.repeat(300), 'ws', 3, 7).title,
		).toHaveLength(255);
	});
	it('handles empty text', () => {
		const d = taskDraftFromMessage(null, 'ws', 3, 7);
		expect(d.title).toBe('');
		expect(d.description).toBe('/ws/chats/3?message=7');
	});
});
