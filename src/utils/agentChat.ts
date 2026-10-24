import type {
	AgentConfirmationEvent,
	AgentConversation,
	AgentMessage,
	AgentQuote,
	AgentReplyEvent,
	AgentStepEvent,
	PendingAction,
	PendingActionStatus,
} from '@/types/agent';
import { normalizePendingActions } from '@/utils/agentConfirmation';
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

const RANK: Record<PendingActionStatus, number> = {
	pending: 0,
	executing: 1,
	executed: 2,
	failed: 2,
	cancelled: 2,
	expired: 2,
	superseded: 2,
};

export const mergePendingActions = (
	current: PendingAction[],
	incoming: PendingAction[],
): PendingAction[] => {
	const incomingById = new Map(incoming.map((a) => [a.id, a]));
	const merged = current.map((cur) => {
		const next = incomingById.get(cur.id);
		return next && RANK[next.status] > RANK[cur.status] ? next : cur;
	});
	const known = new Set(current.map((a) => a.id));
	return [...merged, ...incoming.filter((a) => !known.has(a.id))];
};

const isInFlight = (m: AgentMessage): boolean =>
	m.status === 'pending' || m.status === 'running';

export const applyConversation = (
	s: AgentChatState,
	conversation: AgentConversation,
	messages: AgentMessage[],
): AgentChatState => {
	const pending = messages.find(isInFlight);
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
		quotes?: AgentQuote[];
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
		...(userMessage.quotes?.length ? { quotes: userMessage.quotes } : {}),
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

const replyStats = (e: AgentReplyEvent) => ({
	...(e.duration_ms !== undefined ? { duration_ms: e.duration_ms } : {}),
	...(e.input_tokens !== undefined ? { input_tokens: e.input_tokens } : {}),
	...(e.output_tokens !== undefined ? { output_tokens: e.output_tokens } : {}),
});

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
			...replyStats(e),
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
		...replyStats(e),
	};

	return { ...s, messages, pendingId };
};

export const applyConfirmation = (
	s: AgentChatState,
	e: AgentConfirmationEvent,
): AgentChatState => {
	if (e.conversation_id !== s.conversationId) {
		return s;
	}

	const index = s.messages.findIndex((m) => m.id === e.message_id);
	if (index === -1) {
		return s;
	}

	const message = s.messages[index];
	const actions = mergePendingActions(
		message.pending_actions ?? [],
		normalizePendingActions(e.actions),
	);
	const opensCard =
		isInFlight(message) && actions.some((a) => a.status === 'pending');
	const messages = [...s.messages];
	messages[index] = {
		...message,
		pending_actions: actions,
		status: opensCard ? 'awaiting' : message.status,
	};

	return {
		...s,
		messages,
		pendingId: opensCard && s.pendingId === message.id ? null : s.pendingId,
	};
};

export const applyDecision = (
	s: AgentChatState,
	messageId: number,
	actions: PendingAction[],
): AgentChatState => {
	const index = s.messages.findIndex((m) => m.id === messageId);
	if (index === -1) {
		return s;
	}

	const message = s.messages[index];
	const merged = mergePendingActions(
		message.pending_actions ?? [],
		normalizePendingActions(actions),
	);
	const resumes =
		message.status === 'awaiting' &&
		!merged.some((a) => a.status === 'pending');
	const messages = [...s.messages];
	messages[index] = {
		...message,
		pending_actions: merged,
		status: resumes ? 'running' : message.status,
	};

	return { ...s, messages, pendingId: resumes ? messageId : s.pendingId };
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
	messages.length > 0 && isInFlight(messages[messages.length - 1]);

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
