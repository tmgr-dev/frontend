import {
	cancelPersonaReply,
	deleteMessage,
	editMessage,
	getChat,
	getMessage,
	getMessages,
	getReplies,
	listChats,
	markRead,
	sendMessage,
	type Chat,
	type ChatMention,
	type ChatMessage,
	type ChatMessageHit,
	type ChatMessageStatus,
	type SendMessagePayload,
} from '@/actions/tmgr/chats';
import { usePusher } from '@/composable/usePusher';
import type { ChatEventPayload, ChatEventType } from '@/types/dashboard';
import { ref } from 'vue';

const PAGE_SIZE = 50;
const MARK_READ_DELAY = 500;

export const mergeMessages = (
	list: ChatMessage[],
	incoming: ChatMessage[],
): ChatMessage[] => {
	const byId = new Map(list.map((m) => [m.id, m]));
	incoming.forEach((m) => byId.set(m.id, m));
	return [...byId.values()].sort((a, b) => a.id - b.id);
};

export const markMessageDeleted = (
	list: ChatMessage[],
	messageId: number,
): ChatMessage[] =>
	list.map((m) =>
		m.id === messageId ? { ...m, deleted: true, body: null, files: [] } : m,
	);

export const patchMessageStatus = (
	list: ChatMessage[],
	messageId: number,
	status: ChatMessageStatus,
): ChatMessage[] =>
	list.some((m) => m.id === messageId)
		? list.map((m) => (m.id === messageId ? { ...m, status } : m))
		: list;

export const canEditMessage = (
	message: ChatMessage,
	selfId: number | null,
): boolean =>
	!message.deleted &&
	message.author.kind === 'user' &&
	selfId !== null &&
	message.author.id === selfId;

export const canDeleteMessage = (
	message: ChatMessage,
	selfId: number | null,
	canModerate: boolean,
): boolean =>
	!message.deleted &&
	(canEditMessage(message, selfId) ||
		canModerate ||
		message.author.kind === 'persona');

export const isClosedChannel = (chat: Chat): boolean =>
	!chat.joined && !chat.can_read;

export const formatUnread = (count: number): string =>
	count <= 0 ? '' : count > 99 ? '99+' : String(count);

export const totalUnread = (chats: Chat[]): number =>
	chats.reduce((sum, c) => sum + (c.unread ?? 0), 0);

export const chatTitle = (chat: Chat, selfId: number | null): string => {
	if (chat.kind === 'channel') return chat.title ?? '';
	const others = (chat.members ?? []).filter((m) => m.user_id !== selfId);
	const names = (others.length ? others : (chat.members ?? [])).map(
		(m) => m.name,
	);
	return names.join(', ') || chat.title || '';
};

export const readableHits = (
	hits: ChatMessageHit[],
	chats: Chat[],
): ChatMessageHit[] => {
	const readable = new Set(chats.filter((c) => c.can_read).map((c) => c.id));
	return hits.filter((h) => readable.has(h.chat_id));
};

export const isMuted = (chat: Chat, now = Date.now()): boolean =>
	chat.notify_level === 'none' ||
	(!!chat.muted_until && Date.parse(chat.muted_until) > now);

export interface ChatEventContext {
	openChatId: number | null;
	selfId: number | null;
}

export interface ChatEventResult {
	chats: Chat[];
	reload: boolean;
}

export const applyChatEvent = (
	chats: Chat[],
	type: ChatEventType,
	payload: ChatEventPayload,
	ctx: ChatEventContext,
): ChatEventResult => {
	const known = chats.some((c) => c.id === payload.chat_id);
	const patch = (fn: (c: Chat) => Chat) =>
		chats.map((c) => (c.id === payload.chat_id ? fn(c) : c));

	switch (type) {
		case 'chat.message.created':
		case 'chat.message.updated': {
			if (!known) return { chats, reload: true };
			if (type === 'chat.message.updated') return { chats, reload: false };
			const current = chats.find((c) => c.id === payload.chat_id);
			if (
				!payload.parent_message_id &&
				payload.message_id &&
				current?.last_message &&
				payload.message_id <= current.last_message.id
			)
				return { chats, reload: false };
			if (payload.parent_message_id) return { chats, reload: false };
			const foreign = payload.author_id !== ctx.selfId;
			const unreadCounts = foreign && payload.chat_id !== ctx.openChatId;
			return {
				chats: patch((c) => ({
					...c,
					unread: unreadCounts ? (c.unread ?? 0) + 1 : c.unread,
					last_message_at: new Date().toISOString(),
					last_message: {
						id: payload.message_id ?? 0,
						author_id: payload.author_id ?? null,
						preview: payload.preview ?? null,
						deleted: false,
					},
				})),
				reload: false,
			};
		}
		case 'chat.read':
			return { chats: patch((c) => ({ ...c, unread: 0 })), reload: false };
		case 'chat.member.changed':
			return { chats, reload: true };
		case 'chat.changed':
			return payload.action === 'deleted'
				? {
						chats: chats.filter((c) => c.id !== payload.chat_id),
						reload: false,
					}
				: { chats, reload: true };
		default:
			return { chats, reload: false };
	}
};

const chats = ref<Chat[]>([]);
const chatsLoaded = ref(false);
const openChatId = ref<number | null>(null);
const messages = ref<ChatMessage[]>([]);
const hasOlder = ref(false);
const loadingMessages = ref(false);
const threadRoot = ref<ChatMessage | null>(null);
const replies = ref<ChatMessage[]>([]);
let workspaceId: number | null = null;
let loadGeneration = 0;
let selfId: number | null = null;
let lastReadSent = 0;
let markReadTimer: ReturnType<typeof setTimeout> | null = null;

const topLevel = (list: ChatMessage[]) =>
	list.filter((m) => m.parent_message_id === null);

const lastId = (list: ChatMessage[]) =>
	list.length ? list[list.length - 1].id : 0;

const upsertChat = (chat: Chat) => {
	chats.value = chats.value.some((c) => c.id === chat.id)
		? chats.value.map((c) => (c.id === chat.id ? chat : c))
		: [...chats.value, chat];
};

const removeChat = (chatId: number) => {
	chats.value = chats.value.filter((c) => c.id !== chatId);
};

const loadChats = async (id: number) => {
	workspaceId = id;
	const generation = ++loadGeneration;
	const list = await listChats(id);
	if (generation !== loadGeneration || workspaceId !== id) return;
	chats.value = list;
	chatsLoaded.value = true;
};

const reloadChats = async () => {
	if (workspaceId !== null) await loadChats(workspaceId);
};

const fetchNewer = async () => {
	const chatId = openChatId.value;
	if (chatId === null) return;
	let page: ChatMessage[];
	do {
		page = await getMessages(chatId, {
			after_id: lastId(messages.value),
			limit: PAGE_SIZE,
		});
		if (openChatId.value !== chatId) return;
		messages.value = mergeMessages(messages.value, topLevel(page));
	} while (page.length >= PAGE_SIZE);
};

const fetchNewerReplies = async () => {
	const chatId = openChatId.value;
	const root = threadRoot.value;
	if (chatId === null || !root) return;
	const page = await getReplies(chatId, root.id, {
		after_id: lastId(replies.value),
	});
	if (threadRoot.value?.id !== root.id) return;
	replies.value = mergeMessages(replies.value, page);
};

const refreshRoot = async (chatId: number, rootId: number) => {
	const fresh = await getMessage(chatId, rootId);
	if (openChatId.value !== chatId) return;
	messages.value = mergeMessages(messages.value, [fresh]);
	if (threadRoot.value?.id === rootId) threadRoot.value = fresh;
};

const openChat = async (chatId: number) => {
	openChatId.value = chatId;
	messages.value = [];
	hasOlder.value = false;
	threadRoot.value = null;
	replies.value = [];
	lastReadSent = 0;
	loadingMessages.value = true;
	try {
		if (!chats.value.some((c) => c.id === chatId)) {
			upsertChat(await getChat(chatId));
		}
		const chat = chats.value.find((c) => c.id === chatId);
		if (!chat?.can_read) return;
		const page = await getMessages(chatId, { limit: PAGE_SIZE });
		if (openChatId.value !== chatId) return;
		messages.value = mergeMessages([], topLevel(page));
		hasOlder.value = page.length >= PAGE_SIZE;
	} finally {
		if (openChatId.value === chatId) loadingMessages.value = false;
	}
};

const closeChat = () => {
	openChatId.value = null;
	messages.value = [];
	threadRoot.value = null;
	replies.value = [];
};

const loadOlder = async () => {
	const chatId = openChatId.value;
	if (chatId === null || !hasOlder.value || !messages.value.length) return;
	const page = await getMessages(chatId, {
		before_id: messages.value[0].id,
		limit: PAGE_SIZE,
	});
	if (openChatId.value !== chatId) return;
	messages.value = mergeMessages(messages.value, topLevel(page));
	hasOlder.value = page.length >= PAGE_SIZE;
};

const openThread = async (root: ChatMessage) => {
	const chatId = openChatId.value;
	if (chatId === null) return;
	threadRoot.value = root;
	replies.value = [];
	const page = await getReplies(chatId, root.id);
	if (threadRoot.value?.id !== root.id) return;
	replies.value = mergeMessages([], page);
};

const closeThread = () => {
	threadRoot.value = null;
	replies.value = [];
};

const send = async (chatId: number, payload: SendMessagePayload) => {
	const message = await sendMessage(chatId, payload);
	if (openChatId.value !== chatId) return message;
	if (payload.parent_message_id) {
		replies.value = mergeMessages(replies.value, [message]);
		await refreshRoot(chatId, payload.parent_message_id);
	} else {
		messages.value = mergeMessages(messages.value, [message]);
	}
	return message;
};

const applyMessage = (message: ChatMessage) => {
	if (message.parent_message_id === null) {
		messages.value = mergeMessages(messages.value, [message]);
		if (threadRoot.value?.id === message.id) threadRoot.value = message;
	} else {
		replies.value = mergeMessages(replies.value, [message]);
	}
};

const stopReply = async (message: ChatMessage) => {
	const fresh = await cancelPersonaReply(message.chat_id, message.id);
	if (openChatId.value === message.chat_id) applyMessage(fresh);
};

const removeMessage = async (message: ChatMessage) => {
	await deleteMessage(message.chat_id, message.id);
	if (openChatId.value !== message.chat_id) return;
	if (message.parent_message_id === null) {
		messages.value = markMessageDeleted(messages.value, message.id);
		if (threadRoot.value?.id === message.id) closeThread();
	} else {
		replies.value = markMessageDeleted(replies.value, message.id);
		await refreshRoot(message.chat_id, message.parent_message_id);
	}
};

const saveEdit = async (
	message: ChatMessage,
	body: string,
	mentions: ChatMention[],
) => {
	const fresh = await editMessage(message.chat_id, message.id, {
		body,
		mentions,
	});
	if (openChatId.value === message.chat_id) applyMessage(fresh);
};

const flushMarkRead = async () => {
	markReadTimer = null;
	const chatId = openChatId.value;
	const newest = lastId(messages.value);
	if (chatId === null || !newest || newest <= lastReadSent) return;
	if (typeof document !== 'undefined' && !document.hasFocus()) return;
	lastReadSent = newest;
	try {
		await markRead(chatId, newest);
		chats.value = chats.value.map((c) =>
			c.id === chatId ? { ...c, unread: 0 } : c,
		);
	} catch {
		lastReadSent = 0;
	}
};

const scheduleMarkRead = () => {
	if (markReadTimer) clearTimeout(markReadTimer);
	markReadTimer = setTimeout(flushMarkRead, MARK_READ_DELAY);
};

const markOpenChatRead = async () => {
	lastReadSent = 0;
	await flushMarkRead();
};

const handleEvent = async (type: ChatEventType, payload: ChatEventPayload) => {
	const result = applyChatEvent(chats.value, type, payload, {
		openChatId: openChatId.value,
		selfId,
	});
	chats.value = result.chats;
	if (result.reload) void reloadChats().catch(() => undefined);

	if (payload.chat_id !== openChatId.value) return;
	try {
		if (type === 'chat.message.created' || type === 'chat.message.updated') {
			if (type === 'chat.message.updated' && payload.message_id) {
				if (payload.status) {
					messages.value = patchMessageStatus(
						messages.value,
						payload.message_id,
						payload.status,
					);
					replies.value = patchMessageStatus(
						replies.value,
						payload.message_id,
						payload.status,
					);
				}
				const fresh = await getMessage(payload.chat_id, payload.message_id);
				if (openChatId.value !== payload.chat_id) return;
				if (fresh.parent_message_id === null) {
					messages.value = mergeMessages(messages.value, [fresh]);
				} else if (threadRoot.value?.id === fresh.parent_message_id) {
					replies.value = mergeMessages(replies.value, [fresh]);
				}
			} else if (payload.parent_message_id) {
				await refreshRoot(payload.chat_id, payload.parent_message_id);
				await fetchNewerReplies();
			} else {
				await fetchNewer();
				scheduleMarkRead();
			}
		} else if (type === 'chat.message.deleted' && payload.message_id) {
			if (payload.parent_message_id) {
				replies.value = markMessageDeleted(replies.value, payload.message_id);
				await refreshRoot(payload.chat_id, payload.parent_message_id);
			} else {
				messages.value = markMessageDeleted(messages.value, payload.message_id);
				if (threadRoot.value?.id === payload.message_id) closeThread();
			}
		} else if (type === 'chat.reaction.changed' && payload.message_id) {
			const fresh = await getMessage(payload.chat_id, payload.message_id);
			if (openChatId.value !== payload.chat_id) return;
			if (fresh.parent_message_id === null) {
				messages.value = mergeMessages(messages.value, [fresh]);
			} else if (threadRoot.value?.id === fresh.parent_message_id) {
				replies.value = mergeMessages(replies.value, [fresh]);
			}
			if (threadRoot.value?.id === fresh.id) threadRoot.value = fresh;
		}
	} catch {
		return;
	}
};

const refresh = async () => {
	await reloadChats();
	await fetchNewer();
	await fetchNewerReplies();
	scheduleMarkRead();
};

export const useChats = () => {
	const start = (userId: number, workspace: number) => {
		selfId = userId;
		workspaceId = workspace;
		const channelName = `App.User.${userId}`;
		const subscriptionId = usePusher().subscribeToUser(userId, {
			onChatEvent: handleEvent,
			onReconnect: () => void refresh().catch(() => undefined),
		});
		const onFocus = () => void refresh().catch(() => undefined);
		window.addEventListener('focus', onFocus);
		return () => {
			usePusher().unsubscribeHandler(channelName, subscriptionId);
			window.removeEventListener('focus', onFocus);
			if (markReadTimer) clearTimeout(markReadTimer);
			closeChat();
		};
	};

	return {
		chats,
		chatsLoaded,
		openChatId,
		messages,
		hasOlder,
		loadingMessages,
		threadRoot,
		replies,
		loadChats,
		upsertChat,
		removeChat,
		openChat,
		closeChat,
		loadOlder,
		openThread,
		closeThread,
		send,
		saveEdit,
		stopReply,
		removeMessage,
		scheduleMarkRead,
		markOpenChatRead,
		start,
	};
};
