import $axios from '@/plugins/axios';
import { Workspace } from '@/actions/tmgr/workspaces';
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

export const getStatuses = async (useCache: boolean = true): Promise<Status[]> => {
	const cacheKey = 'statuses';
	
	if (useCache) {
		const cached = requestCache.get<Status[]>(cacheKey);
		if (cached) {
			return cached;
		}
	}

	const {
		data: { data },
	} = await $axios.get('/workspaces/statuses');

	if (useCache) {
		requestCache.set(cacheKey, data, 300000);
	}

	return data;
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
	const {
		data: { data },
	} = await $axios.delete(`/statuses/${statusId}`);

	requestCache.invalidate('statuses');
	requestCache.invalidate('workspace-statuses');
};
