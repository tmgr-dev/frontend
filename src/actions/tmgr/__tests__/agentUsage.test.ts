const state: any = { user: null };
const commit = jest.fn((name: string, value: any) => {
	if (name === 'setUser') state.user = value;
});
const getters = {
	userSettingByKey: (key: string) =>
		state.user?.settings?.find((s: any) => s.key === key),
};
jest.mock('@/store', () => ({
	__esModule: true,
	default: {
		get state() {
			return state;
		},
		getters,
		commit: (...a: any[]) => (commit as any)(...a),
	},
}));
const put = jest.fn();
const get = jest.fn();
const del = jest.fn();
jest.mock('@/plugins/axios', () => ({
	__esModule: true,
	default: {
		put: (...a: any[]) => put(...a),
		get: (...a: any[]) => get(...a),
		delete: (...a: any[]) => del(...a),
	},
}));
jest.mock('../user', () => ({
	updateUserSettingsV2: async (payload: any) => {
		const { data } = await put('v2/user/settings', payload);
		return data.data;
	},
}));

import {
	deleteAllAgentUsage,
	isAgentUsageSyncAvailable,
	isAgentUsageSyncOn,
	listAgentUsage,
	setAgentUsageSync,
	uploadRows,
} from '../agentUsage';

const user = (value: string | null) => ({
	settings: [
		{ id: 5, key: 'current_workspace', value: 56 },
		...(value === null ? [] : [{ id: 9, key: 'agent_usage_sync', value }]),
	],
});

beforeEach(() => {
	commit.mockClear();
	put.mockReset();
	get.mockReset();
	del.mockReset();
});

describe('agent usage sync setting', () => {
	it('is unavailable without the server setting', () => {
		state.user = user(null);
		expect(isAgentUsageSyncAvailable()).toBe(false);
		expect(isAgentUsageSyncOn()).toBe(false);
	});

	it('reads 1 as on and 0 as off', () => {
		state.user = user('1');
		expect(isAgentUsageSyncOn()).toBe(true);
		state.user = user('0');
		expect(isAgentUsageSyncOn()).toBe(false);
	});

	it('puts every setting with the new value for this one', async () => {
		state.user = user('0');
		put.mockResolvedValue({ data: { data: user('1') } });
		await setAgentUsageSync(true);
		expect(put).toHaveBeenCalledWith('v2/user/settings', [
			{ id: 5, value: 56 },
			{ id: 9, value: '1' },
		]);
		expect(isAgentUsageSyncOn()).toBe(true);
	});

	it('rolls back when the request fails', async () => {
		state.user = user('1');
		put.mockRejectedValue(new Error('boom'));
		await expect(setAgentUsageSync(false)).rejects.toThrow('boom');
		expect(isAgentUsageSyncOn()).toBe(true);
	});

	it('throws when the setting does not exist', async () => {
		state.user = user(null);
		await expect(setAgentUsageSync(true)).rejects.toThrow('not available');
		expect(put).not.toHaveBeenCalled();
	});
});

describe('uploads', () => {
	it('lists rows from a bare array or a data envelope', async () => {
		get.mockResolvedValueOnce({ data: [{ task_id: 1 }] });
		expect(await listAgentUsage()).toHaveLength(1);
		get.mockResolvedValueOnce({
			data: { data: [{ task_id: 2 }, { task_id: 3 }] },
		});
		expect(await listAgentUsage()).toHaveLength(2);
		get.mockResolvedValueOnce({ data: null });
		expect(await listAgentUsage()).toEqual([]);
	});

	it('returns the deleted count', async () => {
		del.mockResolvedValue({ data: { deleted: 4 } });
		expect(await deleteAllAgentUsage()).toBe(4);
		expect(del).toHaveBeenCalledWith('agent-usage');
		del.mockResolvedValue({ data: {} });
		expect(await deleteAllAgentUsage()).toBe(0);
	});

	it('formats rows without paths or names beyond the task key', () => {
		const rows = uploadRows([
			{
				task_id: 9975,
				task_key: 'TM-572',
				workspace_id: 56,
				source_id: 'abc',
				generated_at: '2026-10-16T10:00:00Z',
				cost_usd: '18.4',
				events: 10,
				tokens: {
					input: 1000,
					output: 500,
					cache_read: 8500,
					cache_write_5m: 0,
					cache_write_1h: 0,
					reasoning: 0,
				},
				active_ms: 1000,
				updated_at: '2026-10-16T10:00:00Z',
			},
			{
				task_id: 2,
				task_key: null,
				workspace_id: 56,
				source_id: 'abc',
				generated_at: '',
				cost_usd: null,
				events: 0,
				tokens: {
					input: 0,
					output: 0,
					cache_read: 0,
					cache_write_5m: 0,
					cache_write_1h: 0,
					reasoning: 0,
				},
				active_ms: 0,
				updated_at: '',
			},
		]);
		expect(rows[0]).toMatchObject({
			task: 'TM-572',
			cost: '$18.40',
			tokens: '10k tokens',
		});
		expect(rows[1]).toMatchObject({ task: '#2', cost: null });
	});
});
