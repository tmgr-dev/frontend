import $axios from '@/plugins/axios';
import store from '@/store';
import { formatTokens, formatUsd } from '@/usage/taskModel';
import { updateUserSettingsV2 } from './user';

export const AGENT_USAGE_SYNC_KEY = 'agent_usage_sync';

export interface AgentUsageUpload {
	task_id: number;
	task_key: string | null;
	workspace_id: number;
	source_id: string;
	generated_at: string;
	cost_usd: number | string | null;
	events: number;
	tokens: {
		input: number;
		output: number;
		cache_read: number;
		cache_write_5m: number;
		cache_write_1h: number;
		reasoning: number;
	};
	active_ms: number;
	updated_at: string;
}

const isOn = (value: unknown) =>
	value === true || value === 1 || value === '1' || value === 'true';

const withValue = (settings: any[], value: unknown) =>
	settings.map((s: any) =>
		s?.key === AGENT_USAGE_SYNC_KEY ? { ...s, value } : s,
	);

export const isAgentUsageSyncAvailable = (): boolean =>
	!!store.getters.userSettingByKey(AGENT_USAGE_SYNC_KEY);

export const isAgentUsageSyncOn = (): boolean =>
	isOn(store.getters.userSettingByKey(AGENT_USAGE_SYNC_KEY)?.value);

export const withCurrentAgentUsageSync = <
	T extends { id: number; value?: unknown },
>(
	settings: T[],
): T[] => {
	const current = store.getters.userSettingByKey(AGENT_USAGE_SYNC_KEY);
	if (!current) return settings;
	return settings.map((s) =>
		s.id === current.id ? { ...s, value: current.value } : s,
	);
};

export const setAgentUsageSync = async (on: boolean): Promise<void> => {
	const user = (store.state as any).user;
	const setting = store.getters.userSettingByKey(AGENT_USAGE_SYNC_KEY);
	if (!setting || !Array.isArray(user?.settings)) {
		throw new Error('Agent usage sync is not available on this server yet.');
	}
	const value = on ? '1' : '0';
	const payload = user.settings.map((s: any) => ({
		id: s.id,
		value: s.key === AGENT_USAGE_SYNC_KEY ? value : s.value,
	}));
	store.commit('setUser', {
		...user,
		settings: withValue(user.settings, value),
	});
	try {
		store.commit('setUser', await updateUserSettingsV2(payload));
	} catch (error) {
		const current = (store.state as any).user;
		store.commit('setUser', {
			...current,
			settings: withValue(current?.settings ?? [], setting.value),
		});
		throw error;
	}
};

export const listAgentUsage = async (): Promise<AgentUsageUpload[]> => {
	const { data } = await $axios.get('agent-usage');
	const rows = Array.isArray(data) ? data : data?.data;
	return Array.isArray(rows) ? rows : [];
};

export const deleteAllAgentUsage = async (): Promise<number> => {
	const { data } = await $axios.delete('agent-usage');
	const deleted = Number((data?.data ?? data)?.deleted);
	return Number.isFinite(deleted) ? deleted : 0;
};

export interface UploadRow {
	key: string;
	task: string;
	cost: string | null;
	tokens: string;
	updated: string;
}

export const uploadRows = (uploads: AgentUsageUpload[]): UploadRow[] =>
	uploads.map((u) => {
		const t = u.tokens;
		const total =
			t.input + t.output + t.cache_read + t.cache_write_5m + t.cache_write_1h;
		const cost = Number(u.cost_usd);
		return {
			key: `${u.task_id}:${u.source_id}`,
			task: u.task_key || `#${u.task_id}`,
			cost:
				u.cost_usd == null || !Number.isFinite(cost) ? null : formatUsd(cost),
			tokens: `${formatTokens(total)} tokens`,
			updated: u.updated_at,
		};
	});
