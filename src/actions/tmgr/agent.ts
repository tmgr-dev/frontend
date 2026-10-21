import $axios from '@/plugins/axios';
import type {
	AgentContext,
	AgentConversation,
	AgentConversationItem,
	AgentMessage,
} from '@/types/agent';

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

export const getAgentMessages = async (
	conversationId: number,
): Promise<AgentMessage[]> => {
	const {
		data: { data },
	} = await $axios.get(`/agent/conversations/${conversationId}/messages`);
	return data;
};

export const sendAgentMessage = async (
	conversationId: number,
	content: string,
): Promise<{ message_id: number; pending_message_id: number }> => {
	const {
		data: { data },
	} = await $axios.post(`/agent/conversations/${conversationId}/messages`, {
		content,
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
