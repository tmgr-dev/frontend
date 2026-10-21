import $axios from '@/plugins/axios';

export interface TaskCostRate {
	value: string | null;
	source: string;
	hidden: boolean;
}

export interface TaskCostTokens {
	input: number;
	output: number;
	cacheRead?: number;
	cache_read?: number;
	cache_write_5m: number;
	cache_write_1h: number;
	reasoning: number;
}

export interface TaskCostNode {
	source?: number;
	id: string;
	parentId?: string | null;
	parent_id?: string | null;
	depth?: number | null;
	tool: string | null;
	agentType?: string | null;
	agent_type?: string | null;
	model: string | null;
	origin?: string | null;
	link?: string | null;
	durationMs?: number | null;
	duration_ms?: number | null;
	costUsd?: string | null;
	cost_usd?: string | null;
	events?: number | null;
	startedAt?: string | null;
	started_at?: string | null;
	endedAt?: string | null;
	ended_at?: string | null;
	activeMs?: number | null;
	active_ms?: number | null;
	role?: string | null;
	tokens?: TaskCostTokens | null;
	tools?: { name: string; count: number; errors: number }[] | null;
}

export interface TaskCostRates {
	model: string;
	input: string;
	output: string;
	cache_read?: string;
	cacheRead?: string;
	cache_write_5m?: string;
	cache_write_1h?: string;
}

export interface TaskCost {
	taskId: number;
	human: {
		seconds: number;
		timerRunning: boolean;
		rate: TaskCostRate | null;
		cost: string | null;
	};
	agents: {
		workingSeconds: number;
		waitingSeconds: number | null;
		activeSeconds?: number | null;
		wallClockSeconds?: number | null;
		runs: number;
		running: number;
	};
	ai: {
		synced: boolean;
		costUsd: string | null;
		tokens?: TaskCostTokens | null;
		tree?: TaskCostNode[] | null;
		treeTruncated?: boolean;
		tools?: { name: string; count: number; errors: number }[] | null;
		unpricedEvents?: number | null;
		events?: number | null;
		rates?: TaskCostRates[] | null;
	};
	totals: { seconds: number; cost: string | null };
}

export const getTaskCost = async (taskId: number): Promise<TaskCost | null> => {
	try {
		const { data } = await $axios.get(`tasks/${taskId}/cost`);
		const body = data?.data ?? data;
		return body && typeof body === 'object' && body.human ? body : null;
	} catch {
		return null;
	}
};
