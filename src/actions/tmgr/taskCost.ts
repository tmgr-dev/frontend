import $axios from '@/plugins/axios';

export interface TaskCostRate {
	value: string | null;
	source: string;
	hidden: boolean;
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
		runs: number;
		running: number;
	};
	ai: {
		synced: boolean;
		costUsd: string | null;
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
