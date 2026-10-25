import { invoke } from '@tauri-apps/api/core';

export interface UsageRow {
	cost_usd: number;
	tokens_total: number;
	input: number;
	output: number;
	cache_read: number;
	cache_write: number;
	reasoning: number;
	events: number;
	unpriced_events: number;
}

export interface UsageModelRow extends UsageRow {
	tool: string;
	model: string;
}

export interface UsageWindow {
	key: 'today' | 'week' | 'month';
	label: string;
	from: string;
	to: string;
	totals: UsageRow;
	rows: UsageModelRow[];
}

export interface UsageDay {
	day: string;
	tool: string;
	cost_usd: number;
	tokens_total: number;
}

export interface UsageLimit {
	tool: string;
	account: string;
	window: string;
	window_label: string;
	used_pct: number;
	resets_at: string | null;
	resets_in_s: number | null;
	plan: string | null;
	source: 'statusline' | 'rollout';
	observed_at: string;
	age_s: number;
}

export interface UsageOverview {
	supported: boolean;
	unsupported_reason: string | null;
	has_data: boolean;
	history_floor: string | null;
	accounts: { tool: string; label: string }[];
	windows: UsageWindow[];
	days: UsageDay[];
	limits: UsageLimit[];
}

export interface UsageScan {
	state: 'done' | 'busy' | 'off';
	files: number;
	events_new: number;
	events_updated: number;
	errors: number;
	bad_lines: number;
}

export type ConsentState = 'unset' | 'granted' | 'denied';

export interface UsageConsent {
	supported: boolean;
	unsupported_reason: string | null;
	state: ConsentState;
	claude: boolean;
	codex: boolean;
	decided_at: string | null;
	folders: { tool: 'claude' | 'codex'; label: string; path: string }[];
	has_data: boolean;
}

export interface UsageConsentChange {
	state?: 'granted' | 'denied';
	claude?: boolean;
	codex?: boolean;
}

export interface RawRun {
	id: number;
	task_id: number;
	workspace: string;
	status: string | null;
	started_at: string;
	ended_at: string | null;
	duration_seconds: number | null;
	session_id: string | null;
	branch: string | null;
}

export interface UsageTotals {
	cost_usd: number;
	tokens_total: number;
	events: number;
	unpriced_events: number;
}

export interface UsageTaskRow extends UsageTotals {
	task_ref: string;
}

export interface UsageTasks {
	tasks: UsageTaskRow[];
	ambiguous: UsageTotals;
	unattributed: UsageTotals;
	runs_used: number;
	runs_dropped: number;
	runs_truncated: boolean;
}

export const usageScan = (): Promise<UsageScan> => invoke('usage_scan');

export const usageConsent = (): Promise<UsageConsent> =>
	invoke('usage_consent');

export const usageConsentSet = (
	change: UsageConsentChange,
): Promise<UsageConsent> => invoke('usage_consent_set', { ...change });

export const usageForget = (): Promise<UsageConsent> => invoke('usage_forget');

export const usageOverview = (): Promise<UsageOverview> =>
	invoke('usage_overview');

export const usageTasks = (runs: RawRun[]): Promise<UsageTasks> =>
	invoke('usage_tasks', { runs });

export const errorText = (error: unknown): string => {
	if (typeof error === 'string' && error) return error;
	const message = (error as { message?: unknown } | null)?.message;
	return typeof message === 'string' && message
		? message
		: 'Something went wrong.';
};
