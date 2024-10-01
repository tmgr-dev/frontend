import $axios from 'src/plugins/axios';

export interface FormSetting {
	id: number;
	key: string;
	name: string;
	value: string | number;
	description: string;
}

export interface Setting extends Omit<FormSetting, 'value'> {
	component_type: 'time_in_seconds' | 'integer';
	custom_value_available: boolean;
	default_values: any[];
	rules: string;
	variable_type: 'string' | 'integer';
}

export const getTaskSettings = async () => {
	const {
		data: { data },
	} = await $axios.get('tasks/settings');

	return data;
};

export const updateOneTaskSettings = async (
	taskId: number,
	payload: FormSetting,
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
