import $axios from '@/plugins/axios';

export type AgentWorkStatus =
	| 'running'
	| 'succeeded'
	| 'failed'
	| 'cancelled'
	| 'abandoned';

export interface AgentWorkRun {
	id: number;
	task_id: number;
	workspace_id: number;
	user_id: number;
	agent: string;
	model: string | null;
	session_id: string | null;
	branch: string | null;
	status: AgentWorkStatus;
	started_at: string;
	ended_at: string | null;
	duration_seconds: number;
	summary: string | null;
	pr_url: string | null;
	commits: { sha: string; message: string | null }[];
	tests: { passed: number | null; failed: number | null; command: string | null } | null;
	version: number;
}

export interface AgentWorkTotals {
	agent_seconds: number;
	human_seconds: number;
	human_timer_running: boolean;
}

export interface AgentWorkOverview {
	runs: AgentWorkRun[];
	totals: AgentWorkTotals;
}

export const getAgentWork = async (taskId: number): Promise<AgentWorkOverview> => {
	const {
		data: { data },
	} = await $axios.get(`tasks/${taskId}/agent-work`);
	return data;
};
