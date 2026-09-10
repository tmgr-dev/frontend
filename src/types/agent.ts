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
