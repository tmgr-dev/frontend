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

export const getCategorySettings = async () => {
	const {
		data: { data },
	} = await $axios.get('project_categories/settings');

	return data;
};

export const updateCategorySettings = async (
	categoryId: number,
	payload: {},
) => {
	const {
		data: { data },
	} = await $axios.put(`project_categories/${categoryId}/settings`, payload);

	return data.settings;
};
