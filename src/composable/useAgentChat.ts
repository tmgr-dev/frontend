import {
	compactConversation,
	getAgentMessagesWithCompactions,
	getConversation,
	getOrCreateConversation,
	sendAgentMessage,
	startNewConversation,
} from '@/actions/tmgr/agent';
import store from '@/store';
import type {
	AgentCompactedEvent,
	AgentCompaction,
	AgentConversation,
	AgentMessage,
	AgentReplyEvent,
	AgentStepEvent,
} from '@/types/agent';
import type { AgentChatState } from '@/utils/agentChat';
import {
	appendPending,
	applyConversation,
	applyReply,
	applyStep,
	createAgentChatState,
	hasMessage,
	hasPendingLast,
	isBusy,
	isPollResultCurrent,
	lastChatKey,
	parseRememberedChatId,
	resetForWorkspace,
	sendErrorMessage,
} from '@/utils/agentChat';
import {
	canCompactConversation,
	compactErrorMessage,
	mergeCompaction,
} from '@/utils/agentCompaction';
import type { AttachedFile } from '@/utils/aiAttachments';
import { attachmentErrorMessage } from '@/utils/aiAttachments';
import { createRequestSequence } from '@/utils/requestSequence';
import type { ComputedRef, Ref } from 'vue';
import { computed, getCurrentInstance, onUnmounted, ref } from 'vue';
import { usePusher } from './usePusher';

export interface UseAgentChatReturn {
	state: Ref<AgentChatState>;
	loading: Ref<boolean>;
	sending: Ref<boolean>;
	error: Ref<string | null>;
	replyTick: Ref<number>;
	busy: ComputedRef<boolean>;
	compactions: Ref<AgentCompaction[]>;
	compacting: Ref<boolean>;
	canCompact: ComputedRef<boolean>;
	compact: () => Promise<void>;
	load: (workspaceId: number) => Promise<void>;
	openConversation: (conversationId: number) => Promise<void>;
	forgetRemembered: () => void;
	send: (
		content: string,
		scenario?: string,
		fileIds?: number[],
		files?: AttachedFile[],
	) => Promise<boolean>;
	ensureConversation: () => Promise<number | null>;
	newChat: () => Promise<void>;
	subscribe: (userId: number) => void;
	unsubscribe: () => void;
	stopPolling: () => void;
}

const PENDING_POLL_MS = 4000;
const PENDING_POLL_MAX_MS = 10 * 60 * 1000;

const httpStatus = (e: unknown): number | undefined =>
	(e as { response?: { status?: number } } | undefined)?.response?.status;

export function useAgentChat(): UseAgentChatReturn {
	const state = ref<AgentChatState>(createAgentChatState());
	const loading = ref(false);
	const sending = ref(false);
	const error = ref<string | null>(null);
	const replyTick = ref(0);
	const busy = computed(() => isBusy(state.value));
	const compactions = ref<AgentCompaction[]>([]);
	const compacting = ref(false);
	const canCompact = computed(() =>
		canCompactConversation(
			state.value.messages,
			sending.value || loading.value || compacting.value,
		),
	);

	const conversationRequests = createRequestSequence();
	const sendRequests = createRequestSequence();
	const earlyReplies = new Map<number, AgentReplyEvent>();
	const earlySteps = new Map<number, AgentStepEvent[]>();
	let subscriptionId: string | null = null;
	let subscribedUserId: number | null = null;
	let pollTimer: ReturnType<typeof setInterval> | null = null;

	const rememberedKey = (workspaceId: number | null): string | null => {
		const userId = store.state.user?.id;
		return userId && workspaceId ? lastChatKey(userId, workspaceId) : null;
	};

	const readRemembered = (workspaceId: number): number | null => {
		const key = rememberedKey(workspaceId);
		if (!key) return null;
		try {
			return parseRememberedChatId(localStorage.getItem(key));
		} catch {
			return null;
		}
	};

	const remember = (workspaceId: number, conversationId: number): void => {
		const key = rememberedKey(workspaceId);
		if (!key) return;
		try {
			localStorage.setItem(key, String(conversationId));
		} catch {
			return;
		}
	};

	const forget = (workspaceId: number | null): void => {
		const key = rememberedKey(workspaceId);
		if (!key) return;
		try {
			localStorage.removeItem(key);
		} catch {
			return;
		}
	};

	const forgetRemembered = (): void => forget(state.value.workspaceId);

	const stopPolling = (): void => {
		if (pollTimer !== null) clearInterval(pollTimer);
		pollTimer = null;
	};

	const startPolling = (conversationId: number): void => {
		stopPolling();
		let inFlight = false;
		const startedAt = Date.now();
		pollTimer = setInterval(async () => {
			if (inFlight) return;
			if (Date.now() - startedAt > PENDING_POLL_MAX_MS) return stopPolling();
			if (state.value.conversationId !== conversationId) return stopPolling();
			if (!isBusy(state.value)) return stopPolling();
			const pendingId = state.value.pendingId;
			inFlight = true;
			try {
				const history = await getAgentMessagesWithCompactions(conversationId);
				if (!isPollResultCurrent(state.value, conversationId, pendingId))
					return;
				if (hasPendingLast(history.messages)) return;
				state.value = applyConversation(
					state.value,
					{
						id: conversationId,
						workspace_id: state.value.workspaceId as number,
						task_id: null,
						created_at: '',
					},
					history.messages,
				);
				compactions.value = history.compactions;
				replyTick.value++;
				stopPolling();
			} catch {
				return;
			} finally {
				inFlight = false;
			}
		}, PENDING_POLL_MS);
	};

	const showConversation = (
		conversation: AgentConversation,
		messages: AgentMessage[],
		history: AgentCompaction[],
	): void => {
		earlySteps.clear();
		earlyReplies.clear();
		state.value = applyConversation(state.value, conversation, messages);
		compactions.value = history;
		remember(conversation.workspace_id, conversation.id);
		if (hasPendingLast(messages)) startPolling(conversation.id);
		else stopPolling();
	};

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
		replyTick.value++;
	};

	const onAgentCompacted = (e: AgentCompactedEvent): void => {
		if (e.conversation_id !== state.value.conversationId) return;
		compactions.value = mergeCompaction(compactions.value, {
			id: e.compaction_id,
			up_to_message_id: e.up_to_message_id,
			trigger: e.trigger,
			created_at: new Date().toISOString(),
		});
	};

	const load = async (workspaceId: number): Promise<void> => {
		const token = conversationRequests.begin();
		const current = () => conversationRequests.isCurrent(token);
		sendRequests.begin();
		stopPolling();
		sending.value = false;
		earlyReplies.clear();
		earlySteps.clear();
		compactions.value = [];
		state.value = resetForWorkspace(state.value, workspaceId);
		loading.value = true;
		error.value = null;

		try {
			let conversation: AgentConversation | null = null;
			const rememberedId = readRemembered(workspaceId);
			if (rememberedId !== null) {
				try {
					const item = await getConversation(rememberedId);
					if (!current()) return;
					if (
						!item.archived &&
						(item.workspace_id === undefined ||
							item.workspace_id === workspaceId)
					) {
						conversation = {
							id: item.id,
							workspace_id: workspaceId,
							task_id: null,
							created_at: item.created_at,
							title: item.title,
						};
					}
				} catch {
					if (!current()) return;
				}
				if (!conversation) forget(workspaceId);
			}
			if (!conversation) {
				conversation = await getOrCreateConversation(workspaceId);
				if (!current()) return;
			}
			const history = await getAgentMessagesWithCompactions(conversation.id);
			if (!current()) return;
			showConversation(conversation, history.messages, history.compactions);
		} catch (e) {
			if (!current()) return;
			error.value =
				e instanceof Error ? e.message : 'Failed to load conversation';
		} finally {
			if (!current()) return;
			loading.value = false;
		}
	};

	const openConversation = async (conversationId: number): Promise<void> => {
		const workspaceId = state.value.workspaceId;
		if (workspaceId === null) return;
		const token = conversationRequests.begin();
		const current = () => conversationRequests.isCurrent(token);
		sendRequests.begin();
		stopPolling();
		sending.value = false;
		loading.value = true;
		error.value = null;

		try {
			const history = await getAgentMessagesWithCompactions(conversationId);
			if (!current()) return;
			showConversation(
				{
					id: conversationId,
					workspace_id: workspaceId,
					task_id: null,
					created_at: '',
				},
				history.messages,
				history.compactions,
			);
		} catch (e) {
			if (!current()) return;
			const status = httpStatus(e);
			if (status === 403 || status === 404) {
				forget(workspaceId);
				loading.value = false;
				await load(workspaceId);
				return;
			}
			error.value =
				e instanceof Error ? e.message : 'Failed to load conversation';
		} finally {
			if (current()) loading.value = false;
		}
	};

	const ensureConversation = async (): Promise<number | null> => {
		if (state.value.conversationId) return state.value.conversationId;
		const workspaceId = state.value.workspaceId;
		if (workspaceId !== null) await load(workspaceId);
		return state.value.conversationId;
	};

	const send = async (
		content: string,
		scenario?: string,
		fileIds: number[] = [],
		files: AttachedFile[] = [],
	): Promise<boolean> => {
		if (sending.value || loading.value) return false;
		const token = sendRequests.begin();
		const current = () => sendRequests.isCurrent(token);
		const conversationId = state.value.conversationId;
		if (!conversationId) {
			error.value = 'No active conversation';
			return false;
		}

		sending.value = true;
		error.value = null;

		try {
			const { message_id, pending_message_id } = await sendAgentMessage(
				conversationId,
				content,
				scenario,
				fileIds,
			);

			if (!current() || state.value.conversationId !== conversationId) {
				return true;
			}

			state.value = appendPending(
				state.value,
				{
					id: message_id,
					content,
					created_at: new Date().toISOString(),
					files,
				},
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
				replyTick.value++;
				earlyReplies.delete(pending_message_id);
			}
			return true;
		} catch (e) {
			if (!current()) return false;
			error.value =
				attachmentErrorMessage(e) ?? sendErrorMessage(httpStatus(e));
			return false;
		} finally {
			if (current()) sending.value = false;
		}
	};

	const newChat = async (): Promise<void> => {
		if (!state.value.conversationId) {
			return;
		}

		const token = conversationRequests.begin();
		const current = () => conversationRequests.isCurrent(token);
		sendRequests.begin();
		stopPolling();
		sending.value = false;
		loading.value = true;
		error.value = null;

		try {
			const conversation = await startNewConversation(
				state.value.conversationId,
			);
			if (!current()) return;
			showConversation(conversation, [], []);
		} catch (e) {
			if (!current()) return;
			error.value =
				e instanceof Error ? e.message : 'Failed to start new conversation';
		} finally {
			if (!current()) return;
			loading.value = false;
		}
	};

	const compact = async (): Promise<void> => {
		const conversationId = state.value.conversationId;
		if (!conversationId || compacting.value || !canCompact.value) return;

		compacting.value = true;
		error.value = null;

		try {
			const compaction = await compactConversation(conversationId);
			if (state.value.conversationId !== conversationId) return;
			compactions.value = mergeCompaction(compactions.value, compaction);
		} catch (e) {
			if (state.value.conversationId !== conversationId) return;
			error.value = compactErrorMessage(
				httpStatus(e),
				(e as { response?: { data?: { message?: string } } })?.response?.data
					?.message,
			);
		} finally {
			compacting.value = false;
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
		subscriptionId = subscribeToUser(userId, {
			onAgentStep,
			onAgentReply,
			onAgentCompacted,
		});
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
			stopPolling();
			unsubscribe();
		});

	return {
		state,
		loading,
		sending,
		error,
		replyTick,
		busy,
		compactions,
		compacting,
		canCompact,
		compact,
		load,
		openConversation,
		forgetRemembered,
		send,
		ensureConversation,
		newChat,
		subscribe,
		unsubscribe,
		stopPolling,
	};
}
