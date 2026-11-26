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

export interface PermitCardWire {
	session: string;
	agent: string;
	tool: string;
	text: string;
	path?: string;
	reason?: string;
	from?: string;
	warning?: string;
	danger?: string[];
	always_label?: string;
	exp_ms: number;
	nonce?: string;
	desktop_ok?: boolean;
	presence_fps?: string[];
}

export interface PresenceStatus {
	available: boolean;
	set_up: boolean;
	needs_setup_again: boolean;
	key_fp?: string;
	unlocked_secs?: number;
}

export type PermitChoice = 'once' | 'always' | 'deny';

export interface PollOptionWire {
	label: string;
	description?: string;
	recommended?: boolean;
}

export interface PollQuestionWire {
	header: string;
	question: string;
	multi_select?: boolean;
	options: PollOptionWire[];
}

export interface PollWire {
	card_id: string;
	session: string;
	questions?: PollQuestionWire[];
	closed?: string;
	picked?: string;
	can_answer?: boolean;
	why_not?: string;
}

export interface PollPick {
	question_idx: number;
	choices: number[];
	other?: string;
}

export interface DecisionRef {
	card_id: string;
	spawn?: SpawnCardWire;
	permit?: PermitCardWire;
	closed?: string;
	poster?: string;
	live?: boolean;
}

export type MentionState =
	| 'queued'
	| 'sent'
	| 'answering'
	| 'answered'
	| 'busy'
	| 'offline'
	| 'timeout'
	| 'interrupted'
	| 'expired'
	| 'refused'
	| 'rate_limited'
	| 'cancelled'
	| 'other';

export interface MentionRef {
	id: string;
	target: RoomRef;
	target_title: string;
	state: MentionState;
	hop: number;
	reason?: string;
}

export interface MentionReq {
	target: RoomRef;
	confirmed_risky?: boolean;
}

export interface MentionTarget {
	ok: boolean;
	reason?: string;
	risky?: boolean;
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
	mention?: MentionRef;
	poll?: PollWire;
	user_turn?: boolean;
	fresh?: boolean;
}

export interface RoomInfo {
	room: RoomRef;
	title: string;
	agent?: string;
	state?: string;
	context_pct?: number;
	context_tokens?: number;
	model?: string;
	last?: ChatMessage;
	local?: boolean;
	unrestricted?: boolean;
	mention?: MentionTarget;
	task?: string;
	stuck?: string;
}

export interface ToolCall {
	n?: number;
	name: string;
	summary?: string;
	result?: string;
	running?: boolean;
	error?: boolean;
}

export interface ActivityEvent {
	v: number;
	room: RoomRef;
	state: 'working' | 'idle';
	current: string | null;
	used: string[];
	calls?: ToolCall[];
}

export interface ActivityView {
	state: 'working' | 'idle';
	current: string | null;
	used: string[];
	calls?: ToolCall[];
}

export interface HistoryParams {
	room: RoomRef;
	before?: string;
	limit?: number;
}

export interface SendParams {
	id?: string;
	ts_ms?: number;
	room: RoomRef;
	body_md: string;
	reply_to?: string;
	attachments?: Attachment[];
	mention?: MentionReq;
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
	noSandbox?: boolean;
	bypass?: boolean;
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
	gate_blockers?: string[];
	auto_join_error?: string | null;
}

export type PairState =
	| 'paired'
	| 'already'
	| 'pending'
	| 'blocked'
	| 'no_runner'
	| 'no_cli'
	| 'error';

export interface PairResult {
	state: PairState;
	machine?: string;
	device_id?: string;
	message?: string;
	blockers?: string[];
}

export type FollowerState =
	'enrolled' | 'already' | 'refused' | 'unreachable' | 'pending' | 'error';

export interface FollowerInfo {
	follower: string;
	chat_enroll: boolean | null;
}

export interface RemotePolicyLeader {
	machine: string;
	ask: boolean;
	ops: string[];
}

export interface RemotePolicyInfo {
	leaders: RemotePolicyLeader[];
	legacy: boolean;
}

export interface FollowerResult {
	follower: string;
	state: FollowerState;
	machine?: string;
	message?: string;
}

export interface ManagerMachine {
	machine: string;
	label?: string | null;
	role?: 'top' | 'sub' | null;
	followers?: string[];
	local?: boolean;
}

export const RESET_PHRASE = 'RESET';

export type SpawnCardState =
	'issued' | 'approved' | 'claimed' | 'spawned' | 'refused';

export interface SpawnEdit {
	cwd: string;
	mode: string;
	name?: string;
	task?: string;
}

export interface SpawnDecision {
	state: string;
	card_id?: string;
}

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
