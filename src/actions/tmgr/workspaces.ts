import $axios from 'src/plugins/axios';

export const getWorkspaceMembers = async (workspaceId: number) => {
	const {
		data: { data },
	} = await $axios.get(`/workspaces/${workspaceId}/members`);

	return data;
};
