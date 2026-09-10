import type { AgentConversation, AgentMessage, AgentReplyEvent, AgentStepEvent } from '@/types/agent';

export interface AgentChatState {
	conversationId: number | null;
	workspaceId: number | null;
	messages: AgentMessage[];
	pendingId: number | null;
}

export const createAgentChatState = (): AgentChatState => ({
	conversationId: null,
	workspaceId: null,
	messages: [],
	pendingId: null,
});

export const applyConversation = (
	s: AgentChatState,
	conversation: AgentConversation,
	messages: AgentMessage[],
): AgentChatState => {
	const pending = messages.find((m) => m.status === 'pending');

	return {
		conversationId: conversation.id,
		workspaceId: conversation.workspace_id,
		messages,
		pendingId: pending ? pending.id : null,
	};
};

export const appendPending = (
	s: AgentChatState,
	userMessage: { id: number; content: string; created_at: string },
	pendingMessageId: number,
): AgentChatState => {
	const userMsg: AgentMessage = {
		id: userMessage.id,
		role: 'user',
		content: userMessage.content,
		status: 'done',
		steps: [],
		created_at: userMessage.created_at,
	};
	const pendingMsg: AgentMessage = {
		id: pendingMessageId,
		role: 'assistant',
		content: '',
		status: 'pending',
		steps: [],
		created_at: userMessage.created_at,
	};

	return {
		...s,
		messages: [...s.messages, userMsg, pendingMsg],
		pendingId: pendingMessageId,
	};
};

export const applyStep = (s: AgentChatState, e: AgentStepEvent): AgentChatState => {
	if (e.conversation_id !== s.conversationId) {
		return s;
	}

	const index = s.messages.findIndex((m) => m.id === e.message_id);
	if (index === -1) {
		return s;
	}

	const message = s.messages[index];
	const steps = message.steps.some((step) => step.seq === e.seq)
		? message.steps
		: [...message.steps, { seq: e.seq, tool: e.tool, summary: e.summary }].sort((a, b) => a.seq - b.seq);

	const messages = [...s.messages];
	messages[index] = { ...message, steps };

	return { ...s, messages };
};

export const applyReply = (s: AgentChatState, e: AgentReplyEvent): AgentChatState => {
	if (e.conversation_id !== s.conversationId) {
		return s;
	}

	const index = s.messages.findIndex((m) => m.id === e.message_id);
	const pendingId = s.pendingId === e.message_id ? null : s.pendingId;

	if (index === -1) {
		const message: AgentMessage = {
			id: e.message_id,
			role: 'assistant',
			content: e.content,
			status: e.status,
			steps: e.steps,
			created_at: new Date().toISOString(),
		};

		return { ...s, messages: [...s.messages, message], pendingId };
	}

	const existing = s.messages[index];
	const steps = e.steps.length > 0 ? e.steps : existing.steps;
	const messages = [...s.messages];
	messages[index] = { ...existing, status: e.status, content: e.content, steps };

	return { ...s, messages, pendingId };
};

export const hasMessage = (s: AgentChatState, messageId: number): boolean =>
	s.messages.some((m) => m.id === messageId);

export const resetForWorkspace = (s: AgentChatState, workspaceId: number): AgentChatState => {
	if (s.workspaceId === workspaceId) {
		return s;
	}

	return createAgentChatState();
};

export const isBusy = (s: AgentChatState): boolean => s.pendingId !== null;

export const sendErrorMessage = (status: number | undefined): string => {
	switch (status) {
		case 413:
			return 'Your question is too long.';
		case 422:
			return 'Type a question first.';
		case 429:
			return 'The assistant is still answering your previous question.';
		default:
			return 'Could not send your question. Please try again.';
	}
};
