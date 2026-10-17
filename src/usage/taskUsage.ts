import { invoke } from '@tauri-apps/api/core';
import type { RawRun } from './api';

export interface TaskTokens {
	input: number;
	output: number;
	cache_read: number;
	cache_write_5m: number;
	cache_write_1h: number;
	reasoning: number;
}

export interface TaskToolStat {
	name: string;
	count: number;
	errors: number;
	duration_ms: number;
}

export interface TaskNode {
	id: string;
	parent_id: string | null;
	depth: number;
	tool: 'claude' | 'codex';
	agent_type: string;
	model: string;
	origin: string | null;
	link: 'exact' | 'likely' | null;
	started_at: string;
	ended_at: string;
	duration_ms: number;
	cost_usd: number;
	events: number;
	tokens: TaskTokens;
	tools: TaskToolStat[];
}

export interface NamedCount {
	name: string;
	count: number;
}

export interface McpStat {
	server: string;
	tool: string;
	count: number;
	errors: number;
}

export interface HookStat {
	event: string;
	name: string;
	tool: string | null;
	count: number;
	errors: number;
	duration_ms: number;
}

export interface TaskContext {
	tools: TaskToolStat[];
	skills: NamedCount[];
	skills_available: number;
	plugins: NamedCount[];
	mcp: McpStat[];
	hooks: HookStat[];
	slash_commands: NamedCount[];
	permission_modes: { mode: string; count: number }[];
	compactions: number;
	away: number;
	file_edits: { count: number; extensions: NamedCount[] };
	prs: { repo: string; number: number }[];
	agents: {
		tool: 'claude' | 'codex';
		version: string;
		entrypoint: string;
		count: number;
	}[];
}

export interface TaskUsageAggregate {
	schema: string;
	source_id: string;
	generated_at: string;
	task: { workspace: string; ref: string; id: number | null };
	period: { from: string; to: string };
	totals: {
		cost_usd: number;
		events: number;
		unpriced_events: number;
		tokens: TaskTokens;
		active_ms: number;
		waiting_ms?: number;
		turns: number;
		turn_ms: number;
	};
	limits: {
		tool: string;
		window: string;
		used_pct: number;
		approximate: boolean;
	}[];
	nodes: TaskNode[];
	context: TaskContext;
	truncated: { nodes: boolean; lists: boolean };
}

export interface TaskUsage {
	supported: boolean;
	unsupported_reason: string | null;
	aggregate: TaskUsageAggregate | null;
}

export interface TaskUsageQuery {
	runs: RawRun[];
	workspace: string;
	task_id: number;
}

export const usageTask = (query: TaskUsageQuery): Promise<TaskUsage> =>
	invoke('usage_task', {
		runs: query.runs,
		workspace: query.workspace,
		taskId: query.task_id,
	});
