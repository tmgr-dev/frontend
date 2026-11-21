import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import type {
	ChatMessage,
	HistoryParams,
	RoomInfo,
	RoomRef,
	SearchParams,
	SendParams,
	SlashCommand,
} from './types';

export const managerRooms = () => invoke<RoomInfo[]>('manager_rooms');

export const managerHistory = (params: HistoryParams) =>
	invoke<ChatMessage[]>('manager_history', { params });

export const managerSend = (params: SendParams) =>
	invoke<ChatMessage>('manager_send', { params });

export const managerSearch = (params: SearchParams) =>
	invoke<ChatMessage[]>('manager_search', { params });

export const managerCommands = (room: RoomRef) =>
	invoke<SlashCommand[]>('manager_commands', { room });

export const onManagerMessage = (
	handler: (message: ChatMessage) => void,
): Promise<UnlistenFn> =>
	listen<ChatMessage>('manager://message', (event) => handler(event.payload));
