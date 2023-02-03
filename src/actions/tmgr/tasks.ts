import $axios from 'src/plugins/axios';
import { AxiosRequestConfig } from 'axios';

interface Task {}

export const getTasks = async (params: AxiosRequestConfig) => {
	const {
		data: { data },
	} = await $axios.get('tasks/current?all', params);

	return data;
};

export const getTask = async (taskId: number) => {
	const {
		data: { data },
	} = await $axios.get(`tasks/${taskId}`);

	data.common_time = data.common_time || 0;

	return data;
};

export const createTask = async (task: Task) => {
	const {
		data: { data },
	} = await $axios.post('tasks', task);

	return data;
};

export const updateTask = async (taskId: number, task: Task) => {
	const {
		data: { data },
	} = await $axios.put(`tasks/${taskId}`, task);

	return data;
};

export const deleteTask = async (taskId: number) => {
	const {
		data: { data },
	} = await $axios.delete(`/tasks/${taskId}`);

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

export const addTaskAssignee = async (taskId: number, userId: number) => {
	const {
		data: { data },
	} = await $axios.post(`tasks/${taskId}/assign/${userId}`);

	return data.assignees;
};

export const deleteTaskAssignee = async (taskId: number, userId: number) => {
	const {
		data: { data },
	} = await $axios.delete(`tasks/${taskId}/assign/${userId}`);

	return data.assignees;
};

export const startTaskTimeCounter = async (taskId: number) => {
	const {
		data: { data },
	} = await $axios.post(`tasks/${taskId}/countdown`);

	return data;
};

export const stopTaskTimeCounter = async (taskId: number) => {
	const {
		data: { data },
	} = await $axios.delete(`tasks/${taskId}/countdown`);

	return data;
};
