import { AxiosRequestConfig } from 'axios';
import $axios from 'src/plugins/axios';

export const getCategories = async () => {
	const {
		data: { data },
	} = await $axios.get('project_categories?all');

	return data;
};

export const getSubCategories = async (categoryId: number) => {
	const {
		data: { data },
	} = await $axios.get(`project_categories/${categoryId}`);

	return data;
};
