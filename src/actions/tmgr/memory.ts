import { t } from '@/i18n';
import $axios from '@/plugins/axios';
import { requestCache } from '@/utils/requestCache';

export type MemoryTopicKey =
	'people' | 'agreements' | 'decisions' | 'limitations';

export interface MemoryEntry {
	id: string;
	date: string;
	fact: string;
	source: string;
	check_by: string | null;
	stale: boolean;
}

export interface MemoryTopic {
	key: MemoryTopicKey;
	title: string;
	description: string;
	page_id: number | null;
	page_slug: string | null;
	entries: MemoryEntry[];
}

export interface WorkspaceMemory {
	pages_enabled: boolean;
	can_edit: boolean;
	topics: MemoryTopic[];
}

export type ProposalStatus =
	'pending' | 'adding' | 'added' | 'rejected' | 'failed' | 'superseded';

export interface MemoryProposal {
	id: number;
	topic: MemoryTopicKey;
	fact: string;
	source: string | null;
	check_by: string | null;
	origin: 'recap' | 'comment_thread' | 'mcp' | 'summarize';
	status: ProposalStatus;
	auto_add_at: string | null;
	task_id: number | null;
	created_at: string;
	entry_id: string | null;
	kind: 'add' | 'replace' | 'delete';
	target_entry_id: string | null;
	target_text: string | null;
	run_id: number | null;
}

export interface SummarizeParams {
	mode: 'sources' | 'extend';
	days?: number;
	from?: string;
	to?: string;
	category_ids?: number[];
	my_chats: boolean;
	topic?: string;
}

export interface SummarizePreview {
	tasks: number;
	comments: number;
	chats: number;
	skipped: number;
	estimated_tokens: number;
	left_today: number;
	run_cap: number;
}

export interface MemorySummarizeProgressEvent {
	run_id: number;
	stage: string;
	done: number;
	total: number;
}

export interface MemorySummarizeDoneEvent {
	run_id: number;
	status: SummarizeStatus;
	cards: number;
	skipped: number;
	partial: boolean;
}

export type SummarizeStatus =
	'queued' | 'running' | 'done' | 'failed' | 'cancelled' | 'discarded';

export interface SummarizeRun {
	id: number;
	status: SummarizeStatus;
	mode: 'sources' | 'extend';
	stage: string | null;
	progress_done: number;
	progress_total: number;
	tokens_used: number;
	sources_skipped: number;
	error: string | null;
	created_at: string;
	finished_at: string | null;
}

export type RejectResult =
	| { outcome: 'rejected'; proposal: MemoryProposal }
	| { outcome: 'already_added'; proposal: MemoryProposal };

const memoryKey = (workspaceId: number) => `memory-${workspaceId}`;
const proposalsKey = (workspaceId: number) => `memory-proposals-${workspaceId}`;
const base = (workspaceId: number) => `/workspaces/${workspaceId}/memory`;

export const invalidateMemory = (workspaceId?: number) =>
	requestCache.invalidate(
		workspaceId === undefined
			? /^memory-/
			: new RegExp(`^memory(-proposals)?-${workspaceId}$`),
	);

const unwrap = async <T>(request: Promise<{ data: { data: T } }>) =>
	(await request).data.data;

const mutate = async <T>(
	workspaceId: number,
	request: Promise<{ data: { data: T } }>,
): Promise<T> => {
	try {
		return await unwrap(request);
	} finally {
		invalidateMemory(workspaceId);
	}
};

export const getWorkspaceMemory = (
	workspaceId: number,
	useCache = false,
): Promise<WorkspaceMemory> =>
	requestCache.getOrFetch<WorkspaceMemory>(
		memoryKey(workspaceId),
		() => unwrap($axios.get(base(workspaceId))),
		{ ttl: 30000, cache: useCache },
	);

export const getMemoryProposals = (
	workspaceId: number,
	useCache = false,
): Promise<MemoryProposal[]> =>
	requestCache.getOrFetch<MemoryProposal[]>(
		proposalsKey(workspaceId),
		() => unwrap($axios.get(`${base(workspaceId)}/proposals`)),
		{ ttl: 30000, cache: useCache },
	);

export const addMemoryEntry = (
	workspaceId: number,
	topic: MemoryTopicKey,
	fact: string,
): Promise<MemoryEntry> =>
	mutate(workspaceId, $axios.post(base(workspaceId), { topic, fact }));

export const deleteMemoryEntry = (
	workspaceId: number,
	entryId: string,
): Promise<{ deleted: boolean; entry: MemoryEntry }> =>
	mutate(
		workspaceId,
		$axios.delete(
			`${base(workspaceId)}/entries/${encodeURIComponent(entryId)}`,
		),
	);

export const restoreMemoryEntry = (
	workspaceId: number,
	entryId: string,
): Promise<MemoryEntry> =>
	mutate(
		workspaceId,
		$axios.post(`${base(workspaceId)}/entries/restore`, { id: entryId }),
	);

export const acceptMemoryProposal = (
	workspaceId: number,
	proposalId: number,
): Promise<MemoryProposal> =>
	mutate(
		workspaceId,
		$axios.post(`${base(workspaceId)}/proposals/${proposalId}/accept`),
	);

export const rejectMemoryProposal = async (
	workspaceId: number,
	proposalId: number,
): Promise<RejectResult> => {
	try {
		const proposal = await mutate<MemoryProposal>(
			workspaceId,
			$axios.post(`${base(workspaceId)}/proposals/${proposalId}/reject`),
		);
		return { outcome: 'rejected', proposal };
	} catch (error: any) {
		const proposal = error?.response?.data?.data as MemoryProposal | undefined;
		if (error?.response?.status === 409 && proposal?.status === 'added') {
			return { outcome: 'already_added', proposal };
		}
		throw error;
	}
};

export const updateMemoryProposal = (
	workspaceId: number,
	proposalId: number,
	payload: { fact: string; topic?: MemoryTopicKey },
): Promise<MemoryProposal> =>
	mutate(
		workspaceId,
		$axios.put(`${base(workspaceId)}/proposals/${proposalId}`, payload),
	);

export const undoMemoryProposal = (
	workspaceId: number,
	proposalId: number,
): Promise<MemoryProposal> =>
	mutate(
		workspaceId,
		$axios.post(`${base(workspaceId)}/proposals/${proposalId}/undo`),
	);

export const undoAddedToMemory = async (
	workspaceId: number,
	proposalId: number,
): Promise<string> => {
	try {
		await undoMemoryProposal(workspaceId, proposalId);
		return 'Removed from memory';
	} catch (error: any) {
		if (error?.response?.status === 409) return 'Already changed';
		throw error;
	}
};

export const previewSummarize = (
	workspaceId: number,
	params: SummarizeParams,
): Promise<SummarizePreview> =>
	unwrap($axios.post(`${base(workspaceId)}/summarize/preview`, params));

export const startSummarize = (
	workspaceId: number,
	params: SummarizeParams,
): Promise<SummarizeRun> =>
	mutate(workspaceId, $axios.post(`${base(workspaceId)}/summarize`, params));

export const getCurrentSummarizeRun = (
	workspaceId: number,
): Promise<SummarizeRun | null> =>
	unwrap($axios.get(`${base(workspaceId)}/summarize/runs/current`));

export const cancelSummarizeRun = (
	workspaceId: number,
	runId: number,
): Promise<SummarizeRun> =>
	mutate(
		workspaceId,
		$axios.post(`${base(workspaceId)}/summarize/runs/${runId}/cancel`),
	);

export const discardSummarizeRun = (
	workspaceId: number,
	runId: number,
): Promise<SummarizeRun> =>
	mutate(
		workspaceId,
		$axios.post(`${base(workspaceId)}/summarize/runs/${runId}/discard`),
	);

export const getRunProposals = (
	workspaceId: number,
	runId: number,
): Promise<MemoryProposal[]> =>
	unwrap(
		$axios.get(`${base(workspaceId)}/proposals`, { params: { run_id: runId } }),
	);

const errorMessageKeys: Record<string, string> = {
	already_running: 'workspace.workspaceMemory.summarize.alreadyRunning',
	daily_cap: 'workspace.workspaceMemory.summarize.dailyCap',
	no_sources: 'workspace.workspaceMemory.summarize.noSources',
	entry_changed: 'workspace.workspaceMemory.summarize.entryChanged',
};

export const memoryErrorMessage = (error: any, fallback: string): string => {
	const data = error?.response?.data;
	const key = errorMessageKeys[data?.error];
	if (key) return t(key);
	return data?.message || fallback;
};
