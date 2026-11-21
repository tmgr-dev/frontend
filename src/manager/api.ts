import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import type {
	Chat,
	Draft,
	ManagerMessage,
	SearchHit,
	SearchQuery,
	SlashCommand,
} from './types';

export const managerChats = () => invoke<Chat[]>('manager_chats');

export const managerMessages = (
	chatId: string,
	before?: string,
	limit?: number,
) => invoke<ManagerMessage[]>('manager_messages', { chatId, before, limit });

export const managerSend = (chatId: string, draft: Draft) =>
	invoke<ManagerMessage>('manager_send', { chatId, draft });

export const managerCommands = (chatId: string) =>
	invoke<SlashCommand[]>('manager_commands', { chatId });

export const managerSearch = (query: SearchQuery) =>
	invoke<SearchHit[]>('manager_search', { query });

export const managerMarkRead = (chatId: string) =>
	invoke<void>('manager_mark_read', { chatId });

export const onManagerChanged = (
	handler: (chatId: string) => void,
): Promise<UnlistenFn> =>
	listen<string>('manager://changed', (event) => handler(event.payload));
