import $axios from '@/plugins/axios';
import { AxiosRequestConfig } from 'axios';
import store from '@/store';
import objectToQueryString from '@/utils/objectToQueryString';
import { FormSetting } from '@/actions/tmgr/settings';
import { User } from '@/actions/tmgr/user';

export interface Task {
	id: number | undefined;
	approximately_time: number;
	assignees: Record<string, any>[] | number[];
	category: number | {
		id: number;
		title: string;
		code: string;
	};
	title: string;
	status: string;
	description: string | null;
	description_json: Record<string, any> | null;
	common_time: number;
	is_daily_routine: boolean;
	order: number;
	project_category_id?: number | null;
	settings?: FormSetting[];
	start_time: number;
	status_id: number;
	user: Pick<User, 'id' | 'name'>;
	user_id: number;
	workspace_id?: number;
	checkpoints?: Array<{
		description: string;
		start: number;
		end: number;
		checked: boolean;
		inputType: string;
	}>;
	is_recurring?: boolean;
	scheduled_date?: string | null;
	scheduled_time?: { hours: number; minutes: number } | null;
	recurrence?: Record<string, any> | null;
}

interface LinkResponse {
	success: 1 | 0;
	link?: string;
	meta?: {
		title: string;
		description: string;
		image: {
			url: string;
		};
	};
}

export interface PaginationMeta {
	current_page: number;
	per_page: number;
	total: number;
	last_page: number;
	from: number;
	to: number;
}

export interface PaginatedResponse<T> {
	data: T[];
	meta: PaginationMeta;
}

interface GetTasksParams extends AxiosRequestConfig {
	page?: number;
	per_page?: number;
	params?: {
		search?: string | null;
		project_category_id?: number | null;
		[key: string]: any;
	};
}

export const getTasks = async (params: GetTasksParams, current = true): Promise<PaginatedResponse<Task>> => {
	const { data } = await $axios.get(`tasks/${current ? 'current' : ''}`, {
		params: {
			page: params.page,
			per_page: params.per_page,
			...params.params
		}
	});

	return data;
};

export const getTasksIndexes = async (categoryId: null | number = null) => {
	const {
		data: {
			data: { index },
		},
	} = await $axios.get(
		`tasks/indexes${categoryId ? `?category=${categoryId}` : ''}`,
	);

	return index;
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

export const optimizeWithAI = async (text: string) => {
	const {
		data: { data },
	} = await $axios.post('ai/optimize', { text });

	return data;
};

export const updateTask = async (taskId: number, task: Task) => {
	const {
		data: { data },
	} = await $axios.put(`tasks/${taskId}`, task);

	return data;
};

export const updateStatusOfTasks = async (
	taskIds: Array<number>,
	statusId: number,
) => {
	await $axios.put(`statuses/${statusId}/tasks`, { task_ids: taskIds });
};

export const updateTaskPartially = async (taskId: number, task: Task) => {
	const {
		data: { data },
	} = await $axios.patch(`tasks/${taskId}`, task);

	return data;
};

export const deleteTask = async (taskId: number) => {
	const {
		data: { data },
	} = await $axios.delete(`/tasks/${taskId}`);

	/*
	 * reload active tasks because somebody may remove active task without stopping countdown,
	 * and we will need to reload the page to get correct number of active tasks
	 * */
	store.commit('incrementReloadActiveTasksKey');

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
	params: GetTasksParams
): Promise<PaginatedResponse<Task>> => {
	const { data } = await $axios.get(`tasks/status/${statusId}`, {
		params: {
			page: params.page,
			per_page: params.per_page,
			...params.params
		}
	});

	return data;
};

export const getSortedTasksByStatus = async (
	statusId: number,
	params: AxiosRequestConfig,
) => {
	const {
		data: { data },
	} = await $axios.get(`tasks/status/${statusId}?all`, params);

	// @todo simplify it
	return data.sort((a: { order: number }, b: { order: number }) => {
		if (a.order < b.order) {
			return -1;
		}
		if (a.order > b.order) {
			return 1;
		}
		return 0;
	});
};

export const updateTaskStatus = async (
	taskId: number,
	status: string | number,
) => {
	const {
		data: { data },
	} = await $axios.put(`tasks/${taskId}/${status}`);

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

	store.commit('incrementReloadActiveTasksKey'); // reload active tasks

	return data;
};

export const stopTaskTimeCounter = async (taskId: number) => {
	const {
		data: { data },
	} = await $axios.delete(`tasks/${taskId}/countdown`);

	store.commit('incrementReloadActiveTasksKey'); // reload active tasks

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

type taskOrder = {
	id: number;
	order: number;
};

export const updateTaskOrders = async (payload: { tasks: taskOrder[] }) => {
	await $axios.put('/tasks/update-orders', payload);
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

export const fetchLinkMetadata = async (url: string): Promise<LinkResponse> => {
	try {
		const { data } = await $axios.post(`/fetch-link-metadata`, { url });

		return {
			success: 1,
			link: url,
			meta: {
				title: data.title || '',
				description: data.description || '',
				image: {
					url: data.image || '',
				},
			},
		};
	} catch (error) {
		console.error('Error fetching link metadata:', error);
		return {
			success: 0,
		};
	}
};
