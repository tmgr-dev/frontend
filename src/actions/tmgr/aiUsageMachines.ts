import $axios from '@/plugins/axios';

export interface MachineLimit {
	agent: string;
	account: string;
	window: string;
	used_pct: number;
	resets_at: string | null;
	plan: string | null;
	observed_at: string;
	source: string;
}

export interface Machine {
	machine_id: string;
	label: string;
	platform: string;
	cli_version: string | null;
	timezone: string | null;
	last_seen_at: string;
	limits: MachineLimit[];
}

export interface DailyRow {
	day: string;
	machine_id: string;
	agent: string;
	model: string;
	task_id: number | null;
	task_key: string | null;
	workspace_id: number | null;
	input_tokens: number;
	output_tokens: number;
	cache_read_tokens: number;
	cache_write_tokens: number;
	reasoning_tokens: number;
	cost_usd: number;
	sessions: number;
	events: number;
}

export interface DailyQuery {
	from: string;
	to: string;
	machineId?: string;
	agent?: string;
	model?: string;
	taskId?: number;
}

export interface DailyResult {
	rows: DailyRow[];
	truncated: boolean;
}

export const fetchMachines = async (): Promise<Machine[]> => {
	const { data } = await $axios.get('ai-usage/machines');
	const machines = (data?.data ?? data)?.machines;
	return Array.isArray(machines) ? machines : [];
};

export const fetchDaily = async (query: DailyQuery): Promise<DailyResult> => {
	const params: Record<string, string | number> = {
		from: query.from,
		to: query.to,
	};
	if (query.machineId) params.machine_id = query.machineId;
	if (query.agent) params.agent = query.agent;
	if (query.model) params.model = query.model;
	if (query.taskId != null) params.task_id = query.taskId;
	const { data } = await $axios.get('ai-usage/daily', { params });
	const body = data?.data ?? data;
	return {
		rows: Array.isArray(body?.rows) ? body.rows : [],
		truncated: body?.truncated === true,
	};
};

export const removeMachine = async (machineId: string): Promise<void> => {
	await $axios.delete(`ai-usage/machines/${encodeURIComponent(machineId)}`);
};
