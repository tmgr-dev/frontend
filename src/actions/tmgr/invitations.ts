import $axios from '@/plugins/axios';

export interface WorkspaceInvitation {
	id: number;
	workspace_id: number;
	user_id: number;
	token: string;
	email: string | null;
	max_usage_times: number | null;
	expired_at: string | null;
	is_accepted: boolean;
	is_permanent: boolean;
	created_at: string;
	updated_at: string;
}

export const getWorkspaceInvitations = async (
	workspaceId: number
): Promise<WorkspaceInvitation[]> => {
	const {
		data: { data },
	} = await $axios.get(`workspaces/${workspaceId}/invitations`);
	return data;
};

export const revokeInvitation = async (
	workspaceId: number,
	invitationId: number
): Promise<void> => {
	await $axios.delete(`workspaces/${workspaceId}/invitations/${invitationId}`);
};

