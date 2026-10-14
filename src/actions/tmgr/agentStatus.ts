import $axios from '@/plugins/axios';
import type { AgentSession } from '@/utils/agentStatus';

export const getAgentStatus = async (
	workspaceId: number,
): Promise<AgentSession[]> => {
	const { data } = await $axios.get(`workspaces/${workspaceId}/agent-status`);
	const list = Array.isArray(data) ? data : data?.data;
	return Array.isArray(list) ? list : [];
};
