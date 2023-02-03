import $axios from 'src/plugins/axios';

interface Settings {}

export const getTaskSettings = async () => {
	const {
		data: { data },
	} = await $axios.get('tasks/settings');

	return data;
};

export const updateOneTaskSettings = async (
	taskId: number,
	payload: Settings,
) => {
	const {
		data: { data },
	} = await $axios.put(`/tasks/${taskId}/settings`, payload);

	return data;
};
