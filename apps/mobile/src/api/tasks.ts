import { api } from './client';

export type Checkpoint = {
	description: string;
	start: number;
	end: number;
	checked?: boolean;
	inputType?: string;
};

export type Task = {
	id?: number;
	title: string;
	description?: string | null;
	status_id?: number;
	project_category_id?: number | null;
	common_time?: number | null;
	start_time?: number | null;
	checkpoints?: Checkpoint[] | null;
	[key: string]: any;
};

// Active list — backend excludes archived-type statuses.
export async function getCurrentTasks(params: Record<string, any> = {}) {
	const { data } = await api.get('tasks/current', { params });
	return data; // { data, meta }
}

// Board column: all tasks for a status.
export async function getTasksByStatus(
	statusId: number,
	params: Record<string, any> = {},
): Promise<Task[]> {
	const { data } = await api.get(`tasks/status/${statusId}?all`, { params });
	return data?.data ?? [];
}

export async function getTask(id: number): Promise<Task> {
	const { data } = await api.get(`tasks/${id}`);
	return data?.data ?? data;
}

export async function createTask(task: Task): Promise<Task> {
	const { data } = await api.post('tasks', task);
	return data?.data ?? data;
}

// Full update (also how checkpoints are persisted — they live on the task).
export async function updateTask(id: number, task: Task): Promise<Task> {
	const { data } = await api.put(`tasks/${id}`, task);
	return data?.data ?? data;
}

export async function deleteTask(id: number): Promise<void> {
	await api.delete(`tasks/${id}`);
}

// Timer: start/stop tracking (mirrors the web countdown toggle).
export async function toggleCountdown(id: number): Promise<Task> {
	const { data } = await api.post(`tasks/${id}/countdown`);
	return data?.data ?? data;
}

export async function updateStatusOfTasks(taskIds: number[], statusId: number) {
	await api.put(`statuses/${statusId}/tasks`, { task_ids: taskIds });
}
