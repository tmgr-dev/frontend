import $axios from '@/plugins/axios';
import { t } from '@/i18n';
import { requestCache } from '@/utils/requestCache';

export const MEMORY_TOPICS = [
	'decisions',
	'conventions',
	'people',
	'limitations',
	'notes',
] as const;

export type MemoryTopic = (typeof MEMORY_TOPICS)[number];

export interface PersonaMemoryEntry {
	id: number;
	persona_uuid: string;
	kind: 'fact' | 'journal';
	topic: MemoryTopic;
	body: string;
	source_task_id: number | null;
	source_run_id: string | null;
	check_by: string | null;
	author_kind: 'agent' | 'user';
	created_at: string;
	updated_at: string;
	deleted_at?: string | null;
}

export interface PersonaMemoryFilters {
	topic?: string;
	query?: string;
	journal?: boolean;
	limit?: number;
}

export interface PersonaMemoryInput {
	topic: string;
	body: string;
	source_task_id?: number | null;
	check_by?: string | null;
}

export interface WorkspaceMemoryOverviewRow {
	persona: { uuid: string; name: string };
	topics: Record<string, number>;
	recent: PersonaMemoryEntry[];
}

const overviewKey = (workspaceId: number) =>
	`persona-memory-overview-ws-${workspaceId}`;

const invalidateOverview = (workspaceId: number) =>
	requestCache.invalidate(overviewKey(workspaceId));

export const listPersonaMemory = async (
	uuid: string,
	workspaceId: number,
	filters: PersonaMemoryFilters = {},
): Promise<PersonaMemoryEntry[]> => {
	const params: Record<string, unknown> = { workspace_id: workspaceId };
	if (filters.topic) params.topic = filters.topic;
	if (filters.query) params.query = filters.query;
	if (filters.journal) params.journal = true;
	if (filters.limit) params.limit = filters.limit;
	const {
		data: { data },
	} = await $axios.get(`/personas/${uuid}/memory`, { params });
	return data;
};

export const listDeletedPersonaMemory = async (
	uuid: string,
	workspaceId: number,
	limit?: number,
): Promise<PersonaMemoryEntry[]> => {
	const params: Record<string, unknown> = { workspace_id: workspaceId };
	if (limit) params.limit = limit;
	const {
		data: { data },
	} = await $axios.get(`/personas/${uuid}/memory/deleted`, { params });
	return data;
};

export const addPersonaMemory = async (
	uuid: string,
	workspaceId: number,
	input: PersonaMemoryInput,
): Promise<PersonaMemoryEntry> => {
	const {
		data: { data },
	} = await $axios.post(`/personas/${uuid}/memory`, input, {
		params: { workspace_id: workspaceId },
	});
	invalidateOverview(workspaceId);
	return data;
};

export const updatePersonaMemory = async (
	uuid: string,
	workspaceId: number,
	id: number,
	input: { topic?: string; body?: string },
): Promise<PersonaMemoryEntry> => {
	const {
		data: { data },
	} = await $axios.patch(`/personas/${uuid}/memory/${id}`, input, {
		params: { workspace_id: workspaceId },
	});
	invalidateOverview(workspaceId);
	return data;
};

export const forgetPersonaMemory = async (
	uuid: string,
	workspaceId: number,
	id: number,
): Promise<void> => {
	await $axios.delete(`/personas/${uuid}/memory/${id}`, {
		params: { workspace_id: workspaceId },
	});
	invalidateOverview(workspaceId);
};

export const restorePersonaMemory = async (
	uuid: string,
	workspaceId: number,
	id: number,
): Promise<PersonaMemoryEntry> => {
	const {
		data: { data },
	} = await $axios.post(`/personas/${uuid}/memory/${id}/restore`, undefined, {
		params: { workspace_id: workspaceId },
	});
	invalidateOverview(workspaceId);
	return data;
};

export const getSelfMemory = async (uuid: string): Promise<string> => {
	const {
		data: { data },
	} = await $axios.get(`/personas/${uuid}/memory/self`);
	return data?.body ?? '';
};

export const putSelfMemory = async (
	uuid: string,
	body: string,
): Promise<string> => {
	const {
		data: { data },
	} = await $axios.put(`/personas/${uuid}/memory/self`, { body });
	return data?.body ?? body;
};

export const getWorkspaceMemoryOverview = async (
	workspaceId: number,
	useCache = true,
): Promise<WorkspaceMemoryOverviewRow[]> =>
	requestCache.getOrFetch(
		overviewKey(workspaceId),
		async () => {
			const {
				data: { data },
			} = await $axios.get(`/workspaces/${workspaceId}/persona-memory`);
			return data;
		},
		{ ttl: 30000, cache: useCache },
	);

export const memoryErrorMessage = (error: unknown): string => {
	const data = (error as { response?: { data?: any } })?.response?.data;
	if (data?.error === 'memory_secret_detected')
		return t('settingsPages.personaMemory.secretDetected');
	return data?.message || t('settingsPages.personaMemory.generic');
};
