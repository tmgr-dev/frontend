import $axios from 'src/plugins/axios';
import store from 'src/store';
import { AxiosRequestConfig } from 'axios';

export const getTasks = async (params: AxiosRequestConfig) => {
	const {
		data: { data },
	} = await $axios.get('tasks/current?all', params);

	return data;
};

export const getTasksByStatus = async (
	statusId: number,
	params: AxiosRequestConfig,
) => {
	const {
		data: { data },
	} = await $axios.get(`tasks/status/${statusId}?all`, params);

	return data;
};

export const getLaunchedTasks = async () => {
	const {
		data: { data },
	} = await $axios.get('tasks/runned');

	return data;
};

export const getTaskSettings = async () => {
	const {
		data: { data },
	} = await $axios.get('tasks/settings');

	return data;
};
