import $axios from 'src/plugins/axios';
import { Workspace } from 'src/actions/tmgr/workspaces';

export interface Status {
	name: string;
	type: string;
	color: string;
}

export const getStatuses = async (): Promise<Status[]> => {
	const {
		data: { data },
	} = await $axios.get('/workspaces/statuses');

	return data;
};

export const createStatus = async (workspaceId: number, payload: Status) => {
	const {
		data: { data },
	} = await $axios.post(`/workspaces/${workspaceId}/statuses`, payload);

	return data;
};

export const updateStatus = async (statusId: number, payload: Status) => {
	const {
		data: { data },
	} = await $axios.put(`/statuses/${statusId}`, payload);

	return data;
};

export const deleteStatus = async (statusId: number) => {
	const {
		data: { data },
	} = await $axios.delete(`/statuses/${statusId}`);

	return data.deleted_at;
};
