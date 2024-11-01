import { AxiosRequestConfig } from 'axios';
import $axios from 'src/plugins/axios';

export interface Category {
	children_count: number;
	created_at: string;
	deleted_at: null | string;
	id: number;
	parent_category: null | Category;
	project_category_id: null | number;
	settings: any[];
	slug: string;
	tasks_count: number;
	title: string;
	updated_at: string;
	user: {
		id: number;
		name: string;
	};
	user_id: number;
	workspace_id: number;
}

export const getCategories = async (): Promise<Category[]> => {
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

export const createCategory = async (payload: Category) => {
	const {
		data: { data },
	} = await $axios.post('project_categories', payload);

	return data;
};

export const updateCategory = async (categoryId: number, payload: Category) => {
	const {
		data: { data },
	} = await $axios.put(`project_categories/${categoryId}`, payload);

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
