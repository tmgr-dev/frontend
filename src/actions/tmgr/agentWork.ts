import $axios from '@/plugins/axios';

export type AgentWorkStatus =
	'running' | 'succeeded' | 'failed' | 'cancelled' | 'abandoned';

export interface AgentWorkCommit {
	sha: string;
	message: string | null;
	repo?: string | null;
	branch?: string | null;
	source?: 'agent' | 'git' | null;
}

export interface AgentWorkRelease {
	repo: string | null;
	tag: string;
	ci_url: string | null;
	artifact: string | null;
	digest: string | null;
}

export interface AgentWorkDeploy {
	env: string;
	status: string;
	url: string | null;
}

export interface AgentWorkCheck {
	name: string;
	command: string | null;
	result: string;
}

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
	commits: AgentWorkCommit[];
	releases?: AgentWorkRelease[] | null;
	deploy?: AgentWorkDeploy | null;
	checks?: AgentWorkCheck[] | null;
	tests: {
		passed: number | null;
		failed: number | null;
		command: string | null;
	} | null;
	version: number;
	parent_session_id?: string | null;
	role?: string | null;
}

export interface AgentWorkTotals {
	agent_seconds: number;
	human_seconds: number;
	human_timer_running: boolean;
	active_seconds?: number | null;
	wall_clock_seconds?: number | null;
	waiting_seconds?: number | null;
}

export interface AgentWorkOverview {
	runs: AgentWorkRun[];
	totals: AgentWorkTotals;
}

export const getAgentWork = async (
	taskId: number,
): Promise<AgentWorkOverview> => {
	const {
		data: { data },
	} = await $axios.get(`tasks/${taskId}/agent-work`);
	return data;
};
