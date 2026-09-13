import $axios from '@/plugins/axios';
import { requestCache } from '@/utils/requestCache';

export interface FormSetting {
	id: number;
	key: string;
	name: string;
	value: string | number;
	description: string;
}

export interface Setting extends Omit<FormSetting, 'value'> {
	component_type: 'time_in_seconds' | 'integer' | 'select';
	custom_value_available: boolean;
	default_values: any[];
	rules: string;
	variable_type: 'string' | 'integer';
}

export const getTaskSettings = async (useCache: boolean = true) =>
	requestCache.getOrFetch(
		'task-settings',
		async () => {
			const {
				data: { data },
			} = await $axios.get('tasks/settings');
			return data;
		},
		{ ttl: 300000, cache: useCache },
	);

export interface SettingPayload {
	id: number;
	value: string | number;
}

export const updateTaskSettings = async (
	taskId: number,
	settings: SettingPayload[],
) => {
	const {
		data: { data },
	} = await $axios.put(`/tasks/${taskId}/settings`, settings);

	return data;
};

export const getCategorySettings = async (useCache: boolean = true) =>
	requestCache.getOrFetch(
		'category-settings',
		async () => {
			const {
				data: { data },
			} = await $axios.get('project_categories/settings');
			return data;
		},
		{ ttl: 300000, cache: useCache },
	);

export const updateCategorySettings = async (
	categoryId: number,
	payload: {},
) => {
	const {
		data: { data },
	} = await $axios.put(`project_categories/${categoryId}/settings`, payload);

	return data.settings;
};
