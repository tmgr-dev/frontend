// Mirrors chat.v1 from track A (crates/tmgr-ipc/src/chat.rs, schema/chat-message.v1.json, branch feat/tm-1018-manager).
// Replace with A's merged types when they land. SlashCommand is desktop-local (A has no commands method yet).

export type RoomKind = 'manager' | 'session';

export interface RoomRef {
	kind: RoomKind;
	machine: string;
	session?: string;
}

export type AuthorKind = 'owner' | 'manager' | 'session' | 'system';

export interface Author {
	kind: AuthorKind;
	id: string;
	machine?: string;
	device?: string;
	agent?: string;
}

export type MessageKind = 'text' | 'decision' | 'system' | (string & {});

export interface Attachment {
	name: string;
	mime: string;
	size: number;
	sha256: string;
}

export interface SpawnCardWire {
	summary: string;
	request: string;
	nonce: string;
	exp_ms: number;
	target_id: string;
	target_fp: string;
}

export interface DecisionRef {
	card_id: string;
	spawn?: SpawnCardWire;
}

export interface ChatMessage {
	v: 1;
	id: string;
	room: RoomRef;
	author: Author;
	ts_ms: number;
	kind: MessageKind;
	body_md: string;
	attachments?: Attachment[];
	reply_to?: string;
	decision?: DecisionRef;
	user_turn?: boolean;
}

export interface RoomInfo {
	room: RoomRef;
	title: string;
	agent?: string;
	state?: string;
	context_pct?: number;
	context_tokens?: number;
	last?: ChatMessage;
	local?: boolean;
	unrestricted?: boolean;
}

export interface HistoryParams {
	room: RoomRef;
	before?: string;
	limit?: number;
}

export interface SendParams {
	room: RoomRef;
	body_md: string;
	reply_to?: string;
	attachments?: Attachment[];
}

export interface SearchParams {
	query: string;
	room?: RoomRef;
	limit?: number;
}

export interface SlashCommand {
	name: string;
	description: string;
	args?: string;
	group: string;
	dangerous: boolean;
}

export interface AuditEntry {
	ts_ms: number;
	action: string;
	detail: string;
	needs_approval: boolean;
}

export const SPAWN_AGENTS = ['claude', 'codex', 'opencode', 'agy'] as const;
export type SpawnAgent = (typeof SPAWN_AGENTS)[number];

export interface SpawnRequest {
	agent: SpawnAgent;
	machine: string;
	folder: string;
	name: string;
	task: string;
	prompt: string;
}

export const roomKey = (room: RoomRef): string =>
	room.kind === 'manager'
		? `manager:${room.machine}`
		: `session:${room.machine}:${room.session ?? ''}`;

export const MAX_BODY_BYTES = 64 * 1024;
export const MAX_ATTACHMENTS = 16;

export type TrustDeviceState = 'trusted' | 'pending' | 'revoked';

export interface TrustDevice {
	id: string;
	kind: string;
	label: string;
	fp: string;
	state: TrustDeviceState;
	is_self: boolean;
	can_confirm_kind?: boolean;
}

export interface PendingJoin {
	device_id: string;
	label: string;
	fp: string;
	requested_at_ms: number;
	kind?: string;
}

export interface PendingRevoke {
	statement_id: string;
	revoker_label: string;
	revoker_fp: string;
	target_id: string;
	target_label: string;
	target_fp: string;
}

export interface TrustAlert {
	kind: string;
	device_id: string;
	at_ms: number;
}

export interface SeqGap {
	from_device: string;
	expected: number;
	got: number;
}

export interface TrustState {
	registered: boolean;
	self_fp: string;
	devices: TrustDevice[];
	pending_joins: PendingJoin[];
	key_mismatch?: string[];
	pending_revokes?: PendingRevoke[];
	alerts?: TrustAlert[];
	seq_gaps?: SeqGap[];
	joining?: boolean;
	can_join_with_code?: boolean;
}

export const RESET_PHRASE = 'RESET';

export type SpawnCardState =
	'issued' | 'approved' | 'claimed' | 'spawned' | 'refused';

export interface SpawnCardStatus {
	state: SpawnCardState;
	exp_ms?: number;
	session?: string;
	reason?: string;
}

export interface ModelChoice {
	value: string;
	label: string;
}
