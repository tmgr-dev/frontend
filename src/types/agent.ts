export interface AgentStep {
	seq: number;
	tool: string;
	summary: string;
}

export interface AgentMessage {
	id: number;
	role: 'user' | 'assistant';
	content: string;
	status: 'pending' | 'done' | 'failed';
	steps: AgentStep[];
	created_at: string;
}

export interface AgentConversation {
	id: number;
	workspace_id: number;
	task_id: number | null;
	created_at: string;
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

export interface AgentContextTotals {
	input_tokens: number;
	output_tokens: number;
	cache_read_tokens: number;
	cache_write_tokens: number;
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
