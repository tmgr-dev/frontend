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
	TrustState,
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

export const managerTrustState = () =>
	invoke<TrustState>('manager_trust_state');

export const managerJoinDecide = (deviceId: string, accept: boolean) =>
	invoke<void>('manager_join_decide', { deviceId, accept });

export const managerRevokeDecide = (id: string, accept: boolean) =>
	invoke<void>('manager_revoke_decide', { id, accept });

export const managerRevoke = (deviceId: string) =>
	invoke<void>('manager_revoke', { deviceId });

export const managerResetIdentity = () =>
	invoke<void>('manager_reset_identity');

export const onManagerTrust = (handler: () => void): Promise<UnlistenFn> =>
	listen('manager://trust', () => handler());
