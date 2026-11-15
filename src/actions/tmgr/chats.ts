import $axios from '@/plugins/axios';
import { presignUpload, putToStorage } from './files';

export type ChatKind = 'channel' | 'dm' | 'group_dm';
export type ChatVisibility = 'workspace' | 'members';
export type NotifyLevel = 'all' | 'mentions' | 'none';

export interface ChatMember {
	user_id: number;
	name: string;
	role: string;
	joined_at: string | null;
}

export interface ChatLastMessage {
	id: number;
	author_id: number | null;
	preview: string | null;
	deleted: boolean;
}

export interface Chat {
	id: number;
	workspace_id: number | null;
	kind: ChatKind;
	visibility: ChatVisibility;
	title: string | null;
	topic: string | null;
	joined: boolean;
	can_read: boolean;
	can_write: boolean;
	can_moderate: boolean;
	can_delete: boolean;
	archived_at: string | null;
	last_message_at: string | null;
	last_message: ChatLastMessage | null;
	unread: number | null;
	member_count: number;
	members: ChatMember[] | null;
	created_at: string | null;
	notify_level: NotifyLevel | null;
	muted_until: string | null;
}

export interface ChatAuthor {
	kind: string;
	id: number | null;
	name: string | null;
}

export interface ChatMention {
	kind: 'user';
	id: number;
}

export interface ChatReaction {
	emoji: string;
	count: number;
	mine: boolean;
	user_ids: number[];
}

export interface ChatFile {
	id: number;
	name: string;
	size: number | null;
	mime: string | null;
}

export interface ChatMessage {
	id: number;
	chat_id: number;
	author: ChatAuthor;
	body: string | null;
	mentions: ChatMention[];
	parent_message_id: number | null;
	reply_count: number;
	last_reply_at: string | null;
	reactions: ChatReaction[];
	files: ChatFile[];
	edited_at: string | null;
	deleted: boolean;
	created_at: string;
}

export interface ChatMessageHit {
	message_id: number;
	chat_id: number;
	chat_title: string | null;
	chat_kind: ChatKind;
	parent_message_id: number | null;
	author: ChatAuthor;
	snippet: string;
	created_at: string;
}

export interface ChatFileUpload {
	path: string;
	name: string;
	size: number;
	mime: string;
}

export interface SendMessagePayload {
	body: string;
	mentions?: ChatMention[];
	parent_message_id?: number | null;
	files?: ChatFileUpload[];
}

export interface CreateChannelPayload {
	title: string;
	topic?: string | null;
	visibility: ChatVisibility;
	member_ids?: number[];
}

export interface PageParams {
	before_id?: number;
	after_id?: number;
	limit?: number;
}

export interface ChatMessageEvent {
	chat_id: number;
	message_id: number;
	author_id?: number;
	preview?: string | null;
	parent_message_id?: number | null;
}

export interface ChatReadEvent {
	chat_id: number;
	last_read_message_id: number;
}

export interface ChatMemberEvent {
	chat_id: number;
	action: string;
	user_id?: number;
	user_ids?: number[];
}

export interface ChatChangedEvent {
	chat_id: number;
	action: string;
}

const unwrap = async <T>(request: Promise<{ data: { data: T } }>) =>
	(await request).data.data;

const chat = (chatId: number) => `/chats/${chatId}`;
const workspace = (workspaceId: number) => `/workspaces/${workspaceId}/chats`;

export const listChats = (workspaceId: number): Promise<Chat[]> =>
	unwrap($axios.get(workspace(workspaceId)));

export const searchChats = (
	workspaceId: number,
	q: string,
	limit?: number,
): Promise<ChatMessageHit[]> =>
	unwrap(
		$axios.get(`${workspace(workspaceId)}/search`, {
			params: limit === undefined ? { q } : { q, limit },
		}),
	);

export const createChannel = (
	workspaceId: number,
	payload: CreateChannelPayload,
): Promise<Chat> =>
	unwrap($axios.post(workspace(workspaceId), { kind: 'channel', ...payload }));

export const openDm = (workspaceId: number, userIds: number[]): Promise<Chat> =>
	unwrap($axios.post(`${workspace(workspaceId)}/dm`, { user_ids: userIds }));

export const getChat = (chatId: number): Promise<Chat> =>
	unwrap($axios.get(chat(chatId)));

export const updateChat = (
	chatId: number,
	payload: { title?: string; topic?: string | null },
): Promise<Chat> => unwrap($axios.patch(chat(chatId), payload));

export const deleteChat = async (chatId: number): Promise<void> => {
	await $axios.delete(chat(chatId));
};

export const archiveChat = (chatId: number): Promise<Chat> =>
	unwrap($axios.post(`${chat(chatId)}/archive`));

export const unarchiveChat = (chatId: number): Promise<Chat> =>
	unwrap($axios.post(`${chat(chatId)}/unarchive`));

export const getChatMembers = (chatId: number): Promise<ChatMember[]> =>
	unwrap($axios.get(`${chat(chatId)}/members`));

export const addChatMembers = (
	chatId: number,
	userIds: number[],
): Promise<ChatMember[]> =>
	unwrap($axios.post(`${chat(chatId)}/members`, { user_ids: userIds }));

export const removeChatMember = async (
	chatId: number,
	userId: number,
): Promise<void> => {
	await $axios.delete(`${chat(chatId)}/members/${userId}`);
};

export const joinChat = (chatId: number): Promise<Chat> =>
	unwrap($axios.post(`${chat(chatId)}/join`));

export const leaveChat = async (chatId: number): Promise<void> => {
	await $axios.post(`${chat(chatId)}/leave`);
};

export const getMessages = (
	chatId: number,
	params: PageParams = {},
): Promise<ChatMessage[]> =>
	unwrap($axios.get(`${chat(chatId)}/messages`, { params }));

export const getMessage = (
	chatId: number,
	messageId: number,
): Promise<ChatMessage> =>
	unwrap($axios.get(`${chat(chatId)}/messages/${messageId}`));

export const getReplies = (
	chatId: number,
	rootId: number,
	params: PageParams = {},
): Promise<ChatMessage[]> =>
	unwrap($axios.get(`${chat(chatId)}/messages/${rootId}/replies`, { params }));

export const sendMessage = (
	chatId: number,
	payload: SendMessagePayload,
): Promise<ChatMessage> =>
	unwrap($axios.post(`${chat(chatId)}/messages`, payload));

export const editMessage = (
	chatId: number,
	messageId: number,
	payload: { body: string; mentions?: ChatMention[] },
): Promise<ChatMessage> =>
	unwrap($axios.patch(`${chat(chatId)}/messages/${messageId}`, payload));

export const deleteMessage = async (
	chatId: number,
	messageId: number,
): Promise<void> => {
	await $axios.delete(`${chat(chatId)}/messages/${messageId}`);
};

export const toggleReaction = (
	chatId: number,
	messageId: number,
	emoji: string,
): Promise<ChatReaction[]> =>
	unwrap(
		$axios.post(`${chat(chatId)}/messages/${messageId}/reactions/toggle`, {
			emoji,
		}),
	);

export const getFileDownload = (
	chatId: number,
	fileId: number,
): Promise<{ download_url: string; expires_in: number }> =>
	unwrap($axios.get(`${chat(chatId)}/files/${fileId}/download`));

export const markRead = (
	chatId: number,
	messageId: number,
): Promise<{ chat_id: number; last_read_message_id: number }> =>
	unwrap($axios.post(`${chat(chatId)}/read`, { message_id: messageId }));

export const setChatNotifications = (
	chatId: number,
	notifyLevel: NotifyLevel,
	mutedUntil: string | null,
): Promise<Chat> =>
	unwrap(
		$axios.put(`${chat(chatId)}/notifications`, {
			notify_level: notifyLevel,
			muted_until: mutedUntil,
		}),
	);

export const uploadChatFile = async (file: File): Promise<ChatFileUpload> => {
	const target = await presignUpload(file);
	await putToStorage(target, file);
	return {
		path: target.key,
		name: file.name,
		size: file.size,
		mime: target.content_type,
	};
};
