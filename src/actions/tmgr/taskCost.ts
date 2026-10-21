import $axios from '@/plugins/axios';

export interface TaskCostRate {
	value: string | null;
	source: string;
	hidden: boolean;
}

export interface TaskCostTokens {
	input: number;
	output: number;
	cacheRead: number;
	cacheWrite5m: number;
	cacheWrite1h: number;
	reasoning: number;
}

export interface TaskCostTool {
	name: string;
	count: number;
	errors: number;
}

export interface TaskCostNode {
	source?: number;
	id: string;
	parentId?: string | null;
	depth?: number | null;
	tool?: string | null;
	agentType?: string | null;
	model?: string | null;
	origin?: string | null;
	link?: string | null;
	durationMs?: number | null;
	costUsd?: string | null;
	events?: number | null;
	startedAt?: string | null;
	endedAt?: string | null;
	activeMs?: number | null;
	role?: string | null;
	tokens?: TaskCostTokens | null;
	tools?: TaskCostTool[] | null;
}

export interface TaskCostRates {
	model: string;
	input: string;
	output: string;
	cacheRead: string;
	cacheWrite5m: string;
	cacheWrite1h: string;
}

export interface TaskCostAgents {
	workingSeconds: number;
	waitingSeconds: number | null;
	runs: number;
	running: number;
	activeSeconds?: number | null;
	wallClockSeconds?: number | null;
}

export interface TaskCostAi {
	synced: boolean;
	sources: number;
	costUsd: string | null;
	events: number;
	unpricedEvents: number;
	tokens: TaskCostTokens | null;
	activeMs: number;
	lastGeneratedAt: string | null;
	tree: TaskCostNode[] | null;
	treeTruncated: boolean;
	tools: TaskCostTool[] | null;
	rates?: TaskCostRates[] | null;
}

export interface TaskCost {
	taskId: number;
	human: {
		seconds: number;
		timerRunning: boolean;
		rate: TaskCostRate | null;
		cost: string | null;
	};
	agents: TaskCostAgents;
	ai: TaskCostAi;
	totals: { seconds: number; cost: string | null };
}

const camelKey = (key: string): string =>
	key.replace(/_([a-z0-9])/g, (_, c: string) => c.toUpperCase());

export const camelizeKeys = (value: unknown): unknown => {
	if (Array.isArray(value)) return value.map(camelizeKeys);
	if (value && typeof value === 'object') {
		return Object.fromEntries(
			Object.entries(value).map(([key, item]) => [
				camelKey(key),
				camelizeKeys(item),
			]),
		);
	}
	return value;
};

export const normalizeTaskCost = (body: unknown): TaskCost | null => {
	if (!body || typeof body !== 'object' || !(body as any).human) return null;
	return camelizeKeys(body) as TaskCost;
};

export const getTaskCost = async (taskId: number): Promise<TaskCost | null> => {
	try {
		const { data } = await $axios.get(`tasks/${taskId}/cost`);
		return normalizeTaskCost(data?.data ?? data);
	} catch {
		return null;
	}
};
