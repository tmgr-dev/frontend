import $axios from '@/plugins/axios';
import type { AgentConversation, AgentMessage } from '@/types/agent';

export const getOrCreateConversation = async (workspaceId: number): Promise<AgentConversation> => {
	const { data: { data } } = await $axios.post('/agent/conversations', { workspaceId });
	return data;
};

export const startNewConversation = async (conversationId: number): Promise<AgentConversation> => {
	const { data: { data } } = await $axios.post(`/agent/conversations/${conversationId}/new`);
	return data;
};

export const getAgentMessages = async (conversationId: number): Promise<AgentMessage[]> => {
	const { data: { data } } = await $axios.get(`/agent/conversations/${conversationId}/messages`);
	return data;
};

export const sendAgentMessage = async (
	conversationId: number,
	content: string,
): Promise<{ message_id: number; pending_message_id: number }> => {
	const { data: { data } } = await $axios.post(`/agent/conversations/${conversationId}/messages`, { content });
	return data;
};
