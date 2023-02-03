import { AxiosRequestConfig } from 'axios';
import $axios from 'src/plugins/axios';

export const getCategories = async () => {
	const {
		data: { data },
	} = await $axios.get('project_categories?all');

	return data;
};

export const getSubCategories = async (
	categoryId: number | null,
	params: AxiosRequestConfig,
) => {
	const {
		data: { data },
	} = await $axios.get(`project_categories/children/${categoryId}`, params);

	return data;
};

export const getParentCategory = async (categoryId: number) => {
	const {
		data: { data },
	} = await $axios.get(`project_categories/${categoryId}/with/parents`);

	return data;
};

export const getCategory = async (categoryId: number) => {
	const {
		data: { data },
	} = await $axios.get(`project_categories/${categoryId}`);

	return data;
};

export const deleteCategory = async (categoryId: number) => {
	const {
		data: { data },
	} = await $axios.delete(`project_categories/${categoryId}`);

	return data.deleted_at;
};

export const restoreCategory = async (categoryId: number) => {
	const {
		data: { data },
	} = await $axios.post(`project_categories/${categoryId}/restore`);

	return data.deleted_at;
};
