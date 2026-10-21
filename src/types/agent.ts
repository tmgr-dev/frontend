import type { AttachedFile } from '@/utils/aiAttachments';

export interface AgentStep {
	seq: number;
	tool: string;
	summary: string;
}

export type PendingActionStatus =
	| 'pending'
	| 'executing'
	| 'executed'
	| 'failed'
	| 'cancelled'
	| 'expired'
	| 'superseded';

export interface PendingActionPreview {
	title: string;
	fields: { label: string; value: string }[];
	body: string | null;
}

export interface PendingAction {
	id: number;
	tool: string;
	risk: 'low' | 'high';
	status: PendingActionStatus;
	preview: PendingActionPreview;
	result: Record<string, unknown> | null;
	expires_at: string;
}

export interface AgentMessage {
	id: number;
	role: 'user' | 'assistant';
	content: string;
	status: 'pending' | 'awaiting' | 'running' | 'done' | 'failed';
	steps: AgentStep[];
	created_at: string;
	files?: AttachedFile[];
	pending_actions?: PendingAction[];
}

export interface AgentConversation {
	id: number;
	workspace_id: number;
	task_id: number | null;
	created_at: string;
	title?: string | null;
}

export interface AgentConversationItem {
	id: number;
	title: string | null;
	last_message_at: string | null;
	created_at: string;
	archived: boolean;
}

export interface AgentStepEvent {
	conversation_id: number;
	message_id: number;
	task_id?: number | null;
	seq: number;
	tool: string;
	summary: string;
}

export interface AgentReplyEvent {
	conversation_id: number;
	message_id: number;
	task_id?: number | null;
	status: 'done' | 'failed';
	content: string;
	steps: AgentStep[];
}

export interface AgentConfirmationEvent {
	conversation_id: number;
	message_id: number;
	task_id?: number | null;
	actions: PendingAction[];
}

export interface AgentContextTotals {
	input_tokens: number;
	output_tokens: number;
	cache_read_tokens: number;
	cache_write_tokens: number;
}

export type AgentCompactionTrigger = 'manual' | 'auto';

export interface AgentCompaction {
	id: number;
	up_to_message_id: number;
	trigger: AgentCompactionTrigger;
	created_at: string;
}

export interface AgentCompactedEvent {
	conversation_id: number;
	compaction_id: number;
	up_to_message_id: number;
	trigger: AgentCompactionTrigger;
}

export interface AgentContext {
	conversation_id: number;
	provider: string;
	model: string;
	context_tokens: number;
	context_limit: number | null;
	last_message_id: number | null;
	totals: AgentContextTotals;
}
