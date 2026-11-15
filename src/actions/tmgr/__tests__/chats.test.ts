jest.mock('@/plugins/axios', () => ({
	__esModule: true,
	default: {
		get: jest.fn(),
		post: jest.fn(),
		put: jest.fn(),
		patch: jest.fn(),
		delete: jest.fn(),
	},
}));
jest.mock('../files', () => ({
	presignUpload: jest.fn(),
	putToStorage: jest.fn(),
}));

import $axios from '@/plugins/axios';
import { presignUpload, putToStorage } from '../files';
import {
	createChannel,
	getMessage,
	getMessages,
	listChats,
	markRead,
	openDm,
	searchChats,
	sendMessage,
	setChatNotifications,
	toggleReaction,
	uploadChatFile,
} from '../chats';

const ax = $axios as any;
const ok = (data: any) => Promise.resolve({ data: { data } });

beforeEach(() => jest.clearAllMocks());

test('lists chats of a workspace and unwraps data', async () => {
	ax.get.mockReturnValue(ok([{ id: 1 }]));
	await expect(listChats(7)).resolves.toEqual([{ id: 1 }]);
	expect(ax.get).toHaveBeenCalledWith('/workspaces/7/chats');
});

test('search sends q and optional limit', async () => {
	ax.get.mockReturnValue(ok([]));
	await searchChats(7, 'hi');
	expect(ax.get).toHaveBeenCalledWith('/workspaces/7/chats/search', {
		params: { q: 'hi' },
	});
	await searchChats(7, 'hi', 5);
	expect(ax.get).toHaveBeenLastCalledWith('/workspaces/7/chats/search', {
		params: { q: 'hi', limit: 5 },
	});
});

test('creates a channel and opens a dm with snake_case bodies', async () => {
	ax.post.mockReturnValue(ok({ id: 3 }));
	await createChannel(7, { title: 'General', visibility: 'members', member_ids: [2] });
	expect(ax.post).toHaveBeenCalledWith('/workspaces/7/chats', {
		kind: 'channel',
		title: 'General',
		visibility: 'members',
		member_ids: [2],
	});
	await openDm(7, [2, 4]);
	expect(ax.post).toHaveBeenLastCalledWith('/workspaces/7/chats/dm', { user_ids: [2, 4] });
});

test('message routes', async () => {
	ax.get.mockReturnValue(ok([]));
	await getMessages(3, { after_id: 10 });
	expect(ax.get).toHaveBeenCalledWith('/chats/3/messages', { params: { after_id: 10 } });
	ax.get.mockReturnValue(ok({ id: 11 }));
	await expect(getMessage(3, 11)).resolves.toEqual({ id: 11 });
	expect(ax.get).toHaveBeenLastCalledWith('/chats/3/messages/11');

	ax.post.mockReturnValue(ok({ id: 12 }));
	await sendMessage(3, { body: 'hi', mentions: [{ kind: 'user', id: 2 }], parent_message_id: 11 });
	expect(ax.post).toHaveBeenCalledWith('/chats/3/messages', {
		body: 'hi',
		mentions: [{ kind: 'user', id: 2 }],
		parent_message_id: 11,
	});
	await toggleReaction(3, 12, '👍');
	expect(ax.post).toHaveBeenLastCalledWith('/chats/3/messages/12/reactions/toggle', { emoji: '👍' });
	await markRead(3, 12);
	expect(ax.post).toHaveBeenLastCalledWith('/chats/3/read', { message_id: 12 });
});

test('notifications send level and mute', async () => {
	ax.put.mockReturnValue(ok({ id: 3 }));
	await setChatNotifications(3, 'mentions', null);
	expect(ax.put).toHaveBeenCalledWith('/chats/3/notifications', {
		notify_level: 'mentions',
		muted_until: null,
	});
});

test('uploads a file through presign and returns the attach payload', async () => {
	(presignUpload as jest.Mock).mockResolvedValue({ key: 'uploads/1/a.png', content_type: 'image/png' });
	const file = { name: 'a.png', size: 5, type: 'image/png' } as File;
	await expect(uploadChatFile(file)).resolves.toEqual({
		path: 'uploads/1/a.png',
		name: 'a.png',
		size: 5,
		mime: 'image/png',
	});
	expect(putToStorage).toHaveBeenCalled();
});
