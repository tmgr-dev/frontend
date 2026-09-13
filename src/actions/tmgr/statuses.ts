import $axios from '@/plugins/axios';
import { requestCache } from '@/utils/requestCache';

interface StatusPivot {
	workspace_id: number;
	status_id: number;
	is_active: number;
	order: number;
}

export interface Status {
	id: number;
	name: string;
	color: string;
	type: string;
	user_id: number;
	created_at: string;
	updated_at: string;
	pivot: StatusPivot;
}

export const getStatuses = async (
	useCache: boolean = true,
): Promise<Status[]> => {
	const cacheKey = 'statuses';

	return requestCache.getOrFetch<Status[]>(
		cacheKey,
		async () => {
			const {
				data: { data },
			} = await $axios.get('/workspaces/statuses');
			return data;
		},
		{ ttl: 300000, cache: useCache },
	);
};

export const createStatus = async (workspaceId: number, payload: Status) => {
	const {
		data: { data },
	} = await $axios.post(`/workspaces/${workspaceId}/statuses`, payload);

	requestCache.invalidate('statuses');
	requestCache.invalidate('workspace-statuses');

	return data;
};

export const updateStatus = async (statusId: number, payload: Status) => {
	const {
		data: { data },
	} = await $axios.put(`/statuses/${statusId}`, payload);

	requestCache.invalidate('statuses');
	requestCache.invalidate('workspace-statuses');

	return data;
};

export const deleteStatus = async (statusId: number) => {
	await $axios.delete(`/statuses/${statusId}`);

	requestCache.invalidate('statuses');
	requestCache.invalidate('workspace-statuses');
};
