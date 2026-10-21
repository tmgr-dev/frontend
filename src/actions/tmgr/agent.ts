import type { PresignedUpload } from '@/actions/tmgr/files';
import $axios from '@/plugins/axios';
import type {
	AgentCompaction,
	AgentContext,
	AgentConversation,
	AgentConversationItem,
	AgentMessage,
	AgentPin,
	AgentPinLimits,
	PendingAction,
} from '@/types/agent';
import {
	normalizeAgentMessage,
	normalizePendingActions,
} from '@/utils/agentConfirmation';
import { DEFAULT_PIN_LIMITS } from '@/utils/agentPins';
import type { AttachedFile } from '@/utils/aiAttachments';

export const getOrCreateConversation = async (
	workspaceId: number,
): Promise<AgentConversation> => {
	const {
		data: { data },
	} = await $axios.post('/agent/conversations', { workspaceId });
	return data;
};

export const startNewConversation = async (
	conversationId: number,
): Promise<AgentConversation> => {
	const {
		data: { data },
	} = await $axios.post(`/agent/conversations/${conversationId}/new`);
	return data;
};

export const getAgentMessagesWithCompactions = async (
	conversationId: number,
): Promise<{ messages: AgentMessage[]; compactions: AgentCompaction[] }> => {
	const {
		data: { data, compactions },
	} = await $axios.get(`/agent/conversations/${conversationId}/messages`);
	return {
		messages: (data as AgentMessage[]).map(normalizeAgentMessage),
		compactions: compactions ?? [],
	};
};

export const getAgentMessages = async (
	conversationId: number,
): Promise<AgentMessage[]> => {
	const { messages } = await getAgentMessagesWithCompactions(conversationId);
	return messages;
};

export const compactConversation = async (
	conversationId: number,
): Promise<AgentCompaction> => {
	const {
		data: { data },
	} = await $axios.post(`/agent/conversations/${conversationId}/compact`);
	return data;
};

export const getAgentPins = async (
	conversationId: number,
): Promise<{ pins: AgentPin[]; limits: AgentPinLimits }> => {
	const {
		data: { data, limits },
	} = await $axios.get(`/agent/conversations/${conversationId}/pins`);
	return { pins: data ?? [], limits: limits ?? DEFAULT_PIN_LIMITS };
};

export const addAgentPin = async (
	conversationId: number,
	content: string,
	sourceMessageId?: number,
): Promise<AgentPin> => {
	const {
		data: { data },
	} = await $axios.post(`/agent/conversations/${conversationId}/pins`, {
		content,
		...(sourceMessageId ? { source_message_id: sourceMessageId } : {}),
	});
	return data;
};

export const updateAgentPin = async (
	conversationId: number,
	pinId: number,
	content: string,
): Promise<AgentPin> => {
	const {
		data: { data },
	} = await $axios.put(`/agent/conversations/${conversationId}/pins/${pinId}`, {
		content,
	});
	return data;
};

export const deleteAgentPin = async (
	conversationId: number,
	pinId: number,
): Promise<void> => {
	await $axios.delete(`/agent/conversations/${conversationId}/pins/${pinId}`);
};

export type ActionDecision = 'confirm' | 'cancel';

export const decideAgentActions = async (
	messageId: number,
	decision: ActionDecision,
	actionIds: number[],
): Promise<PendingAction[]> => {
	const {
		data: { data },
	} = await $axios.post(`/agent/messages/${messageId}/actions/${decision}`, {
		action_ids: actionIds,
	});
	return normalizePendingActions(data?.actions);
};

export const sendAgentMessage = async (
	conversationId: number,
	content: string,
	scenario?: string,
	fileIds: number[] = [],
): Promise<{ message_id: number; pending_message_id: number }> => {
	const {
		data: { data },
	} = await $axios.post(
		`/agent/conversations/${conversationId}/messages`,
		scenario
			? {
					scenario,
					content,
					timezone: Intl.DateTimeFormat().resolvedOptions().timeZone,
			  }
			: { content, ...(fileIds.length ? { file_ids: fileIds } : {}) },
	);
	return data;
};

export const attachConversationFile = async (
	conversationId: number,
	file: File,
	target: PresignedUpload,
): Promise<AttachedFile> => {
	const {
		data: { data },
	} = await $axios.post(`/agent/conversations/${conversationId}/files`, {
		file_name: file.name,
		file_path: target.key,
		mime_type: target.content_type,
		size_bytes: file.size,
	});
	return data;
};

export const getAgentContext = async (
	conversationId: number,
): Promise<AgentContext> => {
	const {
		data: { data },
	} = await $axios.get(`/agent/conversations/${conversationId}/context`);
	return data;
};

export const listConversations = async (
	workspaceId: number,
	{ q = '', archived = false }: { q?: string; archived?: boolean } = {},
): Promise<AgentConversationItem[]> => {
	const params: Record<string, unknown> = {
		workspaceId,
		archived,
		limit: 50,
	};
	if (q) params.q = q;
	const {
		data: { data },
	} = await $axios.get('/agent/conversations', { params });
	return data;
};

export const getConversation = async (
	conversationId: number,
): Promise<AgentConversationItem & { workspace_id?: number }> => {
	const {
		data: { data },
	} = await $axios.get(`/agent/conversations/${conversationId}`);
	return data;
};

const patchConversation = async (
	conversationId: number,
	body: { title?: string; archived?: boolean },
): Promise<AgentConversationItem> => {
	const {
		data: { data },
	} = await $axios.patch(`/agent/conversations/${conversationId}`, body);
	return data;
};

export const renameConversation = (
	conversationId: number,
	title: string,
): Promise<AgentConversationItem> =>
	patchConversation(conversationId, { title });

export const archiveConversation = (
	conversationId: number,
	archived: boolean,
): Promise<AgentConversationItem> =>
	patchConversation(conversationId, { archived });

export const deleteConversation = async (
	conversationId: number,
): Promise<void> => {
	await $axios.delete(`/agent/conversations/${conversationId}`);
};
