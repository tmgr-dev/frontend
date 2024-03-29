import $axios from 'src/plugins/axios';
import store from 'src/store';

export interface Workspace {
	name: string;
	type: string;
}

export const getWorkspaces = async (): Promise<Workspace[]> => {
	const {
		data: { data },
	} = await $axios.get('workspaces');

	return data;
};

export const getWorkspaceMembers = async (workspaceId: number) => {
	const {
		data: { data },
	} = await $axios.get(`/workspaces/${workspaceId}/members`);

	return data;
};

export const getWorkspaceStatuses = async () => {
	const {
		data: { data },
	} = await $axios.get('workspaces/statuses');

	store.commit('setStatuses', data);

	return data;
};

export const createWorkspace = async (payload: Workspace) => {
	const {
		data: { data },
	} = await $axios.post('workspaces', payload);

	return data;
};

export interface WorkspaceInvitation {
	max_usage_times: string;
	expired_at?: string;
}

export const createWorkspaceInvitation = async (
	workspaceId: number,
	payload: WorkspaceInvitation,
) => {
	const {
		data: { data },
	} = await $axios.post(`/workspaces/${workspaceId}/invitations`, payload);

	return data;
};

export const acceptWorkspaceInvitation = async (token: string) => {
	const {
		data: { data },
	} = await $axios.post(`workspaces/invitations/${token}/accept`);

	return data;
};
