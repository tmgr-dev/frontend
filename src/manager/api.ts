import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import type { Receipt } from './delivery';
import type {
	ActivityEvent,
	AuditEntry,
	ChatMessage,
	FollowerInfo,
	FollowerResult,
	HistoryParams,
	ManagerMachine,
	ModelChoice,
	PairResult,
	RemotePolicyInfo,
	RoomInfo,
	RoomRef,
	SearchParams,
	SendParams,
	SlashCommand,
	SpawnCardStatus,
	SpawnDecision,
	SpawnEdit,
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

export const managerQueued = (room: RoomRef) =>
	invoke<string[]>('manager_queued', { room });

export const managerQueuedWhy = (room: RoomRef) =>
	invoke<Record<string, string>>('manager_queued_why', { room });

export const managerCancel = (room: RoomRef, id: string) =>
	invoke<boolean>('manager_cancel', { room, id });

export const managerMentionInject = (
	room: RoomRef,
	mentionId: string,
	confirmedRisky: boolean,
) =>
	invoke<void>('manager_mention_inject', { room, mentionId, confirmedRisky });

export const managerAgentModels = (agent: string) =>
	invoke<ModelChoice[]>('manager_agent_models', { agent });

export const managerStop = (room: RoomRef, force: boolean) =>
	invoke<{ exited: boolean }>('manager_stop', { room, force });

export const managerRemove = (room: RoomRef) =>
	invoke<void>('manager_remove', { room });

export const spawnCard = (cardId: string) =>
	invoke<SpawnCardStatus>('manager_spawn_card', { cardId });

export const spawnDecide = (
	cardId: string,
	request: string,
	decision: 'approve' | 'reject' | 'edit',
	edit?: SpawnEdit,
) =>
	invoke<SpawnDecision>('manager_spawn_decide', {
		cardId,
		request,
		decision,
		edit,
	});

export const runnerCaps = () =>
	invoke<{ caps: string[] }>('runner_status').then((s) => s.caps);

export const onManagerMessage = (
	handler: (message: ChatMessage) => void,
): Promise<UnlistenFn> =>
	listen<ChatMessage>('manager://message', (event) => handler(event.payload));

export const onManagerActivity = (
	handler: (event: ActivityEvent) => void,
): Promise<UnlistenFn> =>
	listen<ActivityEvent>('manager://activity', (event) =>
		handler(event.payload),
	);

export const onManagerReceipt = async (
	handler: (receipt: Receipt) => void,
): Promise<UnlistenFn> => {
	try {
		return await listen<Receipt>('manager://receipt', (event) =>
			handler(event.payload),
		);
	} catch {
		return () => undefined;
	}
};

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

const commandMissing = (e: unknown) =>
	/not found|unknown command|not allowed|did not find/i.test(
		e instanceof Error ? e.message : String(e),
	);

export const managerPairLocalRunner = async (): Promise<PairResult | null> => {
	try {
		return await invoke<PairResult>('manager_pair_local_runner');
	} catch (e) {
		if (commandMissing(e)) return null;
		throw e;
	}
};

export const managerMachines = async (): Promise<ManagerMachine[]> => {
	try {
		return (await invoke<ManagerMachine[]>('manager_machines')) ?? [];
	} catch (e) {
		if (commandMissing(e)) return [];
		throw e;
	}
};

export const managerFollowers = async (): Promise<FollowerInfo[]> => {
	try {
		return (await invoke<FollowerInfo[]>('manager_followers')) ?? [];
	} catch (e) {
		if (commandMissing(e)) return [];
		throw e;
	}
};

export const managerConnectFollowers = async (): Promise<
	FollowerResult[] | null
> => {
	try {
		return await invoke<FollowerResult[]>('manager_connect_followers');
	} catch (e) {
		if (commandMissing(e)) return null;
		throw e;
	}
};

export const managerRemotePolicy =
	async (): Promise<RemotePolicyInfo | null> => {
		try {
			return await invoke<RemotePolicyInfo>('manager_remote_policy');
		} catch (e) {
			if (commandMissing(e)) return null;
			throw e;
		}
	};

export const managerRemotePolicySet = (leader: string, ask: boolean) =>
	invoke<RemotePolicyInfo>('manager_remote_policy_set', { leader, ask });
