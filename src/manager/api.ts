import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import type {
	AuditEntry,
	ChatMessage,
	HistoryParams,
	RoomInfo,
	RoomRef,
	SearchParams,
	SendParams,
	SlashCommand,
	SpawnRequest,
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

export const onManagerResync = (handler: () => void): Promise<UnlistenFn> =>
	listen('manager://resync', () => handler());

export const managerTrustState = () =>
	invoke<TrustState>('manager_trust_state');

export const managerJoinDecide = (deviceId: string, accept: boolean) =>
	invoke<void>('manager_join_decide', { deviceId, accept });

export const managerRevokeDecide = (id: string, accept: boolean) =>
	invoke<void>('manager_revoke_decide', { id, accept });

export const managerRevoke = (deviceId: string) =>
	invoke<void>('manager_revoke', { deviceId });

export const managerConfirmDeviceKind = (
	deviceId: string,
	kind: 'desktop' | 'mobile',
) => invoke<void>('manager_confirm_device_kind', { deviceId, kind });

export const managerResetIdentity = () =>
	invoke<void>('manager_reset_identity');

export interface JoinCodeView {
	code: string;
	qr: string;
	expires_at_ms: number;
}

export const managerOpenJoinCode = () =>
	invoke<JoinCodeView>('manager_open_join_code');

export const managerCloseJoinCode = (expiresAtMs: number) =>
	invoke<void>('manager_close_join_code', { expiresAtMs });

export const managerJoinWithCode = (code: string) =>
	invoke<void>('manager_join_with_code', { code });

export const onManagerTrust = (handler: () => void): Promise<UnlistenFn> =>
	listen('manager://trust', () => handler());

// STUB: no backend for the manager audit log yet
export const managerAudit = async (): Promise<AuditEntry[]> => [];

// STUB: no backend for starting sessions from the Manager window yet
export const managerSpawnSession = (_req: SpawnRequest): Promise<void> =>
	Promise.reject(
		new Error('Starting sessions from the Manager window is not available yet'),
	);

// STUB: no backend for switching agents from the desktop app yet
export const managerSwitchAgent = (
	_roomKey: string,
	_agent: string,
	_model: string,
): Promise<void> =>
	Promise.reject(
		new Error(
			'Switching agents from the desktop app is not available yet. Use the tmgr CLI.',
		),
	);
