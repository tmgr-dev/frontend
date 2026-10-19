import $axios from '@/plugins/axios';
import type { ApprovalState } from '@/utils/agentApprovalPolicy';

export type ApprovalKind = 'author' | 'confirm';

export interface AgentRunKey {
	key_id: string;
	device_name: string;
	created_at: string;
	fingerprint?: string;
	public_key?: string;
}

export interface TaskApproval {
	state: ApprovalState;
	kind: ApprovalKind | null;
	revoked_reason: string | null;
	revoked_by_kind: string | null;
	revoked_by_name: string | null;
	content_sha256: string;
	setting_enabled: boolean;
	owner_authored: boolean;
	repositories?: string[] | null;
	repos_sha256?: string | null;
}

export interface ApprovalSubmission {
	kind: ApprovalKind;
	key_id: string;
	host: string;
	category_id: number;
	content_sha256: string;
	repos_sha256?: string;
	signed_at: number;
	signature: string;
}

export type AgentRunErrorCode =
	| 'ui_session_required'
	| 'approval_revoked'
	| 'key_unknown'
	| 'hash_mismatch'
	| 'signed_at_skew'
	| 'signature_invalid'
	| 'setting_off'
	| 'category_mismatch'
	| 'confirm_required'
	| 'repositories_changed'
	| 'invalid_repositories';

export const agentRunErrorCode = (error: any): string | null => {
	const body = error?.response?.data;
	return body?.code ?? body?.error?.code ?? body?.data?.code ?? null;
};

export const getAgentRunsSetting = async (
	workspaceId: number | string,
): Promise<boolean> => {
	const { data } = await $axios.get(
		`/workspaces/${workspaceId}/agent-runs/setting`,
	);
	return !!(data?.data ?? data)?.enabled;
};

export const setAgentRunsSetting = async (
	workspaceId: number | string,
	enabled: boolean,
): Promise<boolean> => {
	const { data } = await $axios.put(
		`/workspaces/${workspaceId}/agent-runs/setting`,
		{ enabled },
	);
	return !!(data?.data ?? data)?.enabled;
};

export const registerAgentRunKey = async (
	publicKey: string,
	deviceName: string,
): Promise<AgentRunKey> => {
	const { data } = await $axios.post('/agent-runs/keys', {
		public_key: publicKey,
		device_name: deviceName,
	});
	return data?.data ?? data;
};

export const listAgentRunKeys = async (): Promise<AgentRunKey[]> => {
	const { data } = await $axios.get('/agent-runs/keys');
	return data?.data ?? data ?? [];
};

export const getTaskApproval = async (
	taskId: number | string,
): Promise<TaskApproval> => {
	const { data } = await $axios.get(`/tasks/${taskId}/agent-approval`);
	return data?.data ?? data;
};

export const submitTaskApproval = async (
	taskId: number | string,
	body: ApprovalSubmission,
): Promise<void> => {
	await $axios.post(`/tasks/${taskId}/agent-approval`, body);
};

export const withdrawTaskApproval = async (
	taskId: number | string,
): Promise<void> => {
	await $axios.delete(`/tasks/${taskId}/agent-approval`);
};

export interface BulkApprovalItem extends ApprovalSubmission {
	task_id: number;
}

export interface BulkApprovalResult {
	task_id: number;
	ok: boolean;
	code?: string;
}

export const BULK_MAX = 100;

export const submitBulkApprovals = async (
	items: BulkApprovalItem[],
): Promise<BulkApprovalResult[]> => {
	const results: BulkApprovalResult[] = [];
	for (let i = 0; i < items.length; i += BULK_MAX) {
		const { data } = await $axios.post('/agent-runs/approvals/bulk', {
			items: items.slice(i, i + BULK_MAX),
		});
		results.push(...(data?.data ?? data ?? []));
	}
	return results;
};

export const getCategoryAgentRepositories = async (
	categoryId: number | string,
): Promise<string[]> => {
	const { data } = await $axios.get(
		`/categories/${categoryId}/agent-run-repositories`,
	);
	return (data?.data ?? data)?.repositories ?? [];
};

export const setCategoryAgentRepositories = async (
	categoryId: number | string,
	repositories: string[],
): Promise<string[]> => {
	const { data } = await $axios.put(
		`/categories/${categoryId}/agent-run-repositories`,
		{ repositories },
	);
	return (data?.data ?? data)?.repositories ?? [];
};
