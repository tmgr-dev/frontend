import {
	getAgentMessages,
	getOrCreateConversation,
	sendAgentMessage,
	startNewConversation,
} from '@/actions/tmgr/agent';
import type { AgentReplyEvent, AgentStepEvent } from '@/types/agent';
import type { AgentChatState } from '@/utils/agentChat';
import {
	appendPending,
	applyConversation,
	applyReply,
	applyStep,
	createAgentChatState,
	hasMessage,
	isBusy,
	resetForWorkspace,
	sendErrorMessage,
} from '@/utils/agentChat';
import { createRequestSequence } from '@/utils/requestSequence';
import type { ComputedRef, Ref } from 'vue';
import { computed, getCurrentInstance, onUnmounted, ref } from 'vue';
import { usePusher } from './usePusher';

export interface UseAgentChatReturn {
	state: Ref<AgentChatState>;
	loading: Ref<boolean>;
	sending: Ref<boolean>;
	error: Ref<string | null>;
	busy: ComputedRef<boolean>;
	load: (workspaceId: number) => Promise<void>;
	send: (content: string) => Promise<void>;
	newChat: () => Promise<void>;
	subscribe: (userId: number) => void;
	unsubscribe: () => void;
}

export function useAgentChat(): UseAgentChatReturn {
	const state = ref<AgentChatState>(createAgentChatState());
	const loading = ref(false);
	const sending = ref(false);
	const error = ref<string | null>(null);
	const busy = computed(() => isBusy(state.value));

	const conversationRequests = createRequestSequence();
	const sendRequests = createRequestSequence();
	const earlyReplies = new Map<number, AgentReplyEvent>();
	const earlySteps = new Map<number, AgentStepEvent[]>();
	let subscriptionId: string | null = null;
	let subscribedUserId: number | null = null;

	const isMessageKnown = (messageId: number): boolean =>
		hasMessage(state.value, messageId);

	const onAgentStep = (e: AgentStepEvent): void => {
		if (e.conversation_id !== state.value.conversationId) {
			return;
		}

		if (!isMessageKnown(e.message_id)) {
			const buffered = earlySteps.get(e.message_id) ?? [];
			buffered.push(e);
			earlySteps.set(e.message_id, buffered);
			return;
		}

		state.value = applyStep(state.value, e);
	};

	const onAgentReply = (e: AgentReplyEvent): void => {
		if (e.conversation_id !== state.value.conversationId) {
			return;
		}

		if (!isMessageKnown(e.message_id)) {
			earlyReplies.set(e.message_id, e);
			return;
		}

		state.value = applyReply(state.value, e);
	};

	const load = async (workspaceId: number): Promise<void> => {
		const token = conversationRequests.begin();
		const current = () => conversationRequests.isCurrent(token);
		sendRequests.begin();
		sending.value = false;
		earlyReplies.clear();
		earlySteps.clear();
		state.value = resetForWorkspace(state.value, workspaceId);
		loading.value = true;
		error.value = null;

		try {
			const conversation = await getOrCreateConversation(workspaceId);
			if (!current()) return;
			const messages = await getAgentMessages(conversation.id);
			if (!current()) return;
			earlySteps.clear();
			earlyReplies.clear();
			state.value = applyConversation(state.value, conversation, messages);
		} catch (e) {
			if (!current()) return;
			error.value =
				e instanceof Error ? e.message : 'Failed to load conversation';
		} finally {
			if (!current()) return;
			loading.value = false;
		}
	};

	const send = async (content: string): Promise<void> => {
		if (sending.value || loading.value) return;
		const token = sendRequests.begin();
		const current = () => sendRequests.isCurrent(token);
		const conversationId = state.value.conversationId;
		if (!conversationId) {
			error.value = 'No active conversation';
			return;
		}

		sending.value = true;
		error.value = null;

		try {
			const { message_id, pending_message_id } = await sendAgentMessage(
				conversationId,
				content,
			);

			if (!current() || state.value.conversationId !== conversationId) {
				return;
			}

			state.value = appendPending(
				state.value,
				{ id: message_id, content, created_at: new Date().toISOString() },
				pending_message_id,
			);

			const bufferedSteps = earlySteps.get(pending_message_id);
			if (bufferedSteps) {
				for (const step of bufferedSteps) {
					state.value = applyStep(state.value, step);
				}
				earlySteps.delete(pending_message_id);
			}

			const early = earlyReplies.get(pending_message_id);
			if (early) {
				state.value = applyReply(state.value, early);
				earlyReplies.delete(pending_message_id);
			}
		} catch (e) {
			if (!current()) return;
			const status = (e as { response?: { status?: number } } | undefined)
				?.response?.status;
			error.value = sendErrorMessage(status);
		} finally {
			if (!current()) return;
			sending.value = false;
		}
	};

	const newChat = async (): Promise<void> => {
		if (!state.value.conversationId) {
			return;
		}

		const token = conversationRequests.begin();
		const current = () => conversationRequests.isCurrent(token);
		sendRequests.begin();
		sending.value = false;
		loading.value = true;
		error.value = null;

		try {
			const conversation = await startNewConversation(
				state.value.conversationId,
			);
			if (!current()) return;
			earlySteps.clear();
			earlyReplies.clear();
			state.value = applyConversation(state.value, conversation, []);
		} catch (e) {
			if (!current()) return;
			error.value =
				e instanceof Error ? e.message : 'Failed to start new conversation';
		} finally {
			if (!current()) return;
			loading.value = false;
		}
	};

	const subscribe = (userId: number): void => {
		if (subscribedUserId === userId && subscriptionId !== null) {
			return;
		}
		if (subscribedUserId !== null) {
			unsubscribe();
		}
		const { subscribeToUser } = usePusher();
		subscribedUserId = userId;
		subscriptionId = subscribeToUser(userId, { onAgentStep, onAgentReply });
	};

	const unsubscribe = (): void => {
		if (subscriptionId !== null && subscribedUserId !== null) {
			const { unsubscribeHandler } = usePusher();
			unsubscribeHandler(`App.User.${subscribedUserId}`, subscriptionId);
		}
		subscriptionId = null;
		subscribedUserId = null;
		earlySteps.clear();
		earlyReplies.clear();
	};

	if (getCurrentInstance())
		onUnmounted(() => {
			conversationRequests.dispose();
			sendRequests.dispose();
			unsubscribe();
		});

	return {
		state,
		loading,
		sending,
		error,
		busy,
		load,
		send,
		newChat,
		subscribe,
		unsubscribe,
	};
}
