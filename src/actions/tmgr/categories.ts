import { AxiosRequestConfig } from 'axios';
import $axios from '@/plugins/axios';
import { requestCache } from '@/utils/requestCache';

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

export interface PaginatedResponse<T> {
	data: T[];
	meta: {
		current_page: number;
		from: number;
		last_page: number;
		path: string;
		per_page: number;
		to: number;
		total: number;
	};
	links: {
		first: string;
		last: string;
		prev: string | null;
		next: string | null;
	};
}

export const getCategories = async (useCache: boolean = true): Promise<Category[]> => {
	const cacheKey = 'categories';
	
	if (useCache) {
		const cached = requestCache.get<Category[]>(cacheKey);
		if (cached) {
			return cached;
		}
	}

	const {
		data: { data },
	} = await $axios.get('project_categories?all');

	if (useCache) {
		requestCache.set(cacheKey, data, 300000);
	}

	return data;
};

export const getTopCategories = async (): Promise<Category[]> => {
	const {
		data: { data },
	} = await $axios.get('project_categories/children/?all=');

	return data;
};

export const getSubCategories = async (
	categoryId: number | null,
	params: AxiosRequestConfig,
): Promise<PaginatedResponse<Category>> => {
	const { data } = await $axios.get(`project_categories/children/${categoryId}`, params);

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

	requestCache.invalidate('categories');

	return data;
};

export const updateCategory = async (categoryId: number, payload: Category) => {
	const {
		data: { data },
	} = await $axios.put(`project_categories/${categoryId}`, payload);

	requestCache.invalidate('categories');

	return data;
};

export const deleteCategory = async (categoryId: number) => {
	const {
		data: { data },
	} = await $axios.delete(`project_categories/${categoryId}`);

	requestCache.invalidate('categories');

	return data.deleted_at;
};

export const restoreCategory = async (categoryId: number) => {
	const {
		data: { data },
	} = await $axios.post(`project_categories/${categoryId}/restore`);

	requestCache.invalidate('categories');

	return data.deleted_at;
};
