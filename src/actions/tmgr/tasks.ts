import $axios from 'src/plugins/axios';
import { AxiosRequestConfig } from 'axios';
import objectToQueryString from 'src/utils/objectToQueryString';

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

	return data.deleted_at;
};

export const restoreDeletedTask = async (taskId: number) => {
	const {
		data: { data },
	} = await $axios.post(`/tasks/${taskId}/restore`);

	return data.deleted_at;
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

export const updateTaskStatus = async (
	taskId: number,
	status: string | number,
) => {
	const {
		data: { data },
	} = await $axios.get(`tasks/${taskId}/${status}`);

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

export const updateTaskTimeCounter = async (
	taskId: number,
	payload: { common_time: number },
) => {
	const {
		data: { data },
	} = await $axios.put(`tasks/${taskId}/time`, payload);

	return data;
};

type exportType = 'csv' | 'jpg' | 'xlsx';

export const exportTasks = async (
	exportType: exportType,
	params: { ids: number[]; per_hour: number },
) => {
	const { data } = await $axios.get(
		`exports/tasks/${exportType}?${objectToQueryString(params)}`,
		{
			responseType: 'blob',
		},
	);

	return data;
};
