import type {
	AgentConversation,
	AgentMessage,
	AgentReplyEvent,
	AgentStepEvent,
} from '@/types/agent';
import type { AttachedFile } from '@/utils/aiAttachments';

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
	const known = new Map(s.messages.map((m) => [m.id, m]));

	return {
		conversationId: conversation.id,
		workspaceId: conversation.workspace_id,
		messages: messages.map((m) => {
			const previous = known.get(m.id);
			return previous && JSON.stringify(previous) === JSON.stringify(m)
				? previous
				: m;
		}),
		pendingId: pending ? pending.id : null,
	};
};

export const appendPending = (
	s: AgentChatState,
	userMessage: {
		id: number;
		content: string;
		created_at: string;
		files?: AttachedFile[];
	},
	pendingMessageId: number,
): AgentChatState => {
	const userMsg: AgentMessage = {
		id: userMessage.id,
		role: 'user',
		content: userMessage.content,
		status: 'done',
		steps: [],
		created_at: userMessage.created_at,
		...(userMessage.files?.length ? { files: userMessage.files } : {}),
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

export const applyStep = (
	s: AgentChatState,
	e: AgentStepEvent,
): AgentChatState => {
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
		: [...message.steps, { seq: e.seq, tool: e.tool, summary: e.summary }].sort(
				(a, b) => a.seq - b.seq,
		  );

	const messages = [...s.messages];
	messages[index] = { ...message, steps };

	return { ...s, messages };
};

export const applyReply = (
	s: AgentChatState,
	e: AgentReplyEvent,
): AgentChatState => {
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
	messages[index] = {
		...existing,
		status: e.status,
		content: e.content,
		steps,
	};

	return { ...s, messages, pendingId };
};

export const hasMessage = (s: AgentChatState, messageId: number): boolean =>
	s.messages.some((m) => m.id === messageId);

export const resetForWorkspace = (
	s: AgentChatState,
	workspaceId: number,
): AgentChatState => {
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

export const lastChatKey = (userId: number, workspaceId: number): string =>
	`tmgr.agentChat.last.${userId}.${workspaceId}`;

export const parseRememberedChatId = (raw: string | null): number | null => {
	if (raw === null || !/^\d+$/.test(raw)) return null;
	const id = Number(raw);
	return id > 0 ? id : null;
};

export const hasPendingLast = (messages: AgentMessage[]): boolean =>
	messages.length > 0 && messages[messages.length - 1].status === 'pending';

export const isPollResultCurrent = (
	s: AgentChatState,
	conversationId: number,
	pendingId: number | null,
): boolean =>
	pendingId !== null &&
	s.conversationId === conversationId &&
	s.pendingId === pendingId;

export const nextChatAfterRemoval = (
	items: { id: number }[],
	removedId: number,
): number | null => {
	const index = items.findIndex((i) => i.id === removedId);
	const rest = items.filter((i) => i.id !== removedId);
	if (!rest.length) return null;
	return rest[Math.min(Math.max(index, 0), rest.length - 1)].id;
};

export const chatTitle = (title: string | null | undefined): string =>
	title && title.trim() ? title : 'New chat';

export const formatChatTime = (
	iso: string | null,
	nowMs: number = Date.now(),
): string => {
	if (!iso) return '';
	const at = new Date(iso).getTime();
	if (Number.isNaN(at)) return '';
	const minutes = Math.max(0, Math.floor((nowMs - at) / 60000));
	if (minutes < 1) return 'just now';
	if (minutes < 60) return `${minutes} min ago`;
	const hours = Math.floor(minutes / 60);
	if (hours < 24) return `${hours} h ago`;
	return new Date(at).toLocaleDateString('en-US', {
		month: 'short',
		day: 'numeric',
		...(new Date(at).getFullYear() === new Date(nowMs).getFullYear()
			? {}
			: { year: 'numeric' }),
	});
};
