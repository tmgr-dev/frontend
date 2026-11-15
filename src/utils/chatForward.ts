import type { Chat, ChatMessage, ForwardDto } from '@/actions/tmgr/chats';

const TITLE_MAX = 255;

export const isOpenChannel = (chat: Chat | null | undefined): boolean =>
	!!chat && chat.kind === 'channel' && chat.visibility === 'workspace';

const isSettled = (message: ChatMessage): boolean =>
	!message.deleted && (!message.status || message.status === 'done');

export const canForwardMessage = (
	message: ChatMessage,
	chat: Chat | null | undefined,
): boolean => isOpenChannel(chat) && isSettled(message);

export const canCreateTaskFromMessage = canForwardMessage;

export const forwardSnapshotLabel = (forward: ForwardDto): string =>
	forward.key || forward.title || '';

export interface ForwardCardView {
	deleted: boolean;
	edited: boolean;
	text: string;
	fileNames: string[];
}

export const forwardCardView = (forward: ForwardDto): ForwardCardView => {
	if (forward.state === 'deleted')
		return { deleted: true, edited: false, text: '', fileNames: [] };
	return {
		deleted: false,
		edited: forward.state === 'edited',
		text: forward.text ?? '',
		fileNames: forward.file_names ?? [],
	};
};

export const forwardErrorKey = (error: unknown): string => {
	const code = (error as { response?: { data?: { error?: unknown } } } | null)
		?.response?.data?.error;
	if (code === 'forward_not_allowed') return 'chats.forward.notAllowed';
	if (code === 'forward_source_not_found')
		return 'chats.forward.sourceNotFound';
	return 'chats.forward.failed';
};

export const taskLink = (workspaceCode: string, taskId: number): string =>
	`/${workspaceCode}/tasks/${taskId}`;

export const messageLink = (
	workspaceCode: string,
	chatId: number,
	messageId: number,
): string => `/${workspaceCode}/chats/${chatId}?message=${messageId}`;

export const taskDraftFromMessage = (
	text: string | null | undefined,
	workspaceCode: string,
	chatId: number,
	messageId: number,
): { title: string; description: string } => {
	const body = text ?? '';
	const first =
		body
			.split('\n')
			.map((line) => line.trim())
			.find((line) => line) ?? '';
	const link = messageLink(workspaceCode, chatId, messageId);
	return {
		title: first.slice(0, TITLE_MAX),
		description: body ? `${body}\n\n${link}` : link,
	};
};
