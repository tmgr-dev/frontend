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
  hasMessage,
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
  const earlySteps = new Map<number, AgentStepEvent[]>();
  let subscriptionId: string | null = null;
  let subscribedUserId: number | null = null;

  const isMessageKnown = (messageId: number): boolean => hasMessage(state.value, messageId);

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
    state.value = resetForWorkspace(state.value, workspaceId);
    loading.value = true;
    error.value = null;

    try {
      const conversation = await getOrCreateConversation(workspaceId);
      const messages = await getAgentMessages(conversation.id);
      earlySteps.clear();
      earlyReplies.clear();
      state.value = applyConversation(state.value, conversation, messages);
    } catch (e) {
      error.value = e instanceof Error ? e.message : 'Failed to load conversation';
    } finally {
      loading.value = false;
    }
  };

  const send = async (content: string): Promise<void> => {
    const conversationId = state.value.conversationId;
    if (!conversationId) {
      error.value = 'No active conversation';
      return;
    }

    sending.value = true;
    error.value = null;

    try {
      const { message_id, pending_message_id } = await sendAgentMessage(conversationId, content);

      if (state.value.conversationId !== conversationId) {
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
      earlySteps.clear();
      earlyReplies.clear();
      state.value = applyConversation(state.value, conversation, []);
    } catch (e) {
      error.value = e instanceof Error ? e.message : 'Failed to start new conversation';
    } finally {
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
