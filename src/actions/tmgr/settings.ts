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

export const getTaskSettings = async (useCache: boolean = true) => {
	const cacheKey = 'task-settings';
	
	if (useCache) {
		const cached = requestCache.get(cacheKey);
		if (cached) {
			return cached;
		}
	}

	const {
		data: { data },
	} = await $axios.get('tasks/settings');

	requestCache.set(cacheKey, data, 300000);

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

export const getCategorySettings = async (useCache: boolean = true) => {
	const cacheKey = 'category-settings';
	
	if (useCache) {
		const cached = requestCache.get(cacheKey);
		if (cached) {
			return cached;
		}
	}

	const {
		data: { data },
	} = await $axios.get('project_categories/settings');

	requestCache.set(cacheKey, data, 300000);

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
