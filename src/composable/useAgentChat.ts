import { ref, computed } from 'vue';
import type { Ref, ComputedRef } from 'vue';
import {
  getAgentMessages,
  getOrCreateConversation,
  sendAgentMessage,
  startNewConversation,
} from '@/actions/tmgr/agent';
import type { AgentReplyEvent, AgentStepEvent } from '@/types/agent';
import {
  appendPending,
  applyConversation,
  applyReply,
  applyStep,
  createAgentChatState,
  isBusy,
  resetForWorkspace,
} from '@/utils/agentChat';
import type { AgentChatState } from '@/utils/agentChat';
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

  const earlyReplies = new Map<number, AgentReplyEvent>();
  let subscriptionId: string | null = null;
  let subscribedUserId: number | null = null;

  const onAgentStep = (e: AgentStepEvent): void => {
    state.value = applyStep(state.value, e);
  };

  const onAgentReply = (e: AgentReplyEvent): void => {
    const known =
      state.value.pendingId === e.message_id ||
      state.value.messages.some((m) => m.id === e.message_id);

    if (!known) {
      earlyReplies.set(e.message_id, e);
      return;
    }

    state.value = applyReply(state.value, e);
  };

  const load = async (workspaceId: number): Promise<void> => {
    state.value = resetForWorkspace(state.value, workspaceId);
    loading.value = true;
    error.value = null;

    try {
      const conversation = await getOrCreateConversation(workspaceId);
      const messages = await getAgentMessages(conversation.id);
      state.value = applyConversation(state.value, conversation, messages);
    } catch (e) {
      error.value = e instanceof Error ? e.message : 'Failed to load conversation';
    } finally {
      loading.value = false;
    }
  };

  const send = async (content: string): Promise<void> => {
    if (!state.value.conversationId) {
      error.value = 'No active conversation';
      return;
    }

    sending.value = true;
    error.value = null;

    try {
      const { message_id, pending_message_id } = await sendAgentMessage(
        state.value.conversationId,
        content,
      );
      state.value = appendPending(
        state.value,
        { id: message_id, content, created_at: new Date().toISOString() },
        pending_message_id,
      );

      const early = earlyReplies.get(pending_message_id);
      if (early) {
        state.value = applyReply(state.value, early);
        earlyReplies.delete(pending_message_id);
      }
    } catch (e) {
      error.value = e instanceof Error ? e.message : 'Failed to send message';
    } finally {
      sending.value = false;
    }
  };

  const newChat = async (): Promise<void> => {
    if (!state.value.conversationId) {
      return;
    }

    loading.value = true;
    error.value = null;

    try {
      const conversation = await startNewConversation(state.value.conversationId);
      state.value = applyConversation(state.value, conversation, []);
    } catch (e) {
      error.value = e instanceof Error ? e.message : 'Failed to start new conversation';
    } finally {
      loading.value = false;
    }
  };

  const subscribe = (userId: number): void => {
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
  };

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
