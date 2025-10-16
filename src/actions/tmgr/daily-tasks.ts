import $axios from '@/plugins/axios';
import { Task } from '@/actions/tmgr/tasks';

export const getDailyTasks = async (): Promise<Task[]> => {
	try {
		const {
			data: { data },
		} = await $axios.get(`daily-routines/tasks`);

		return data;
	} catch (error) {
		console.error('Failed to fetch daily tasks:', error);
		throw error;
	}
};

export const getArchivedDailyTasksCount = async (): Promise<Number> => {
	try {
		const {
			data: { count },
		} = await $axios.get(`daily-routines/tasks/archived/count`);
		return count;
	} catch (error) {
		console.error('Failed to fetch archived daily tasks count:', error);
		throw error;
	}
};

export const getDailyTasksCount = async (): Promise<Number> => {
	try {
		const {
			data: { count },
		} = await $axios.get(`daily-routines/tasks/count`);
		return count;
	} catch (error) {
		console.error('Failed to fetch archived daily tasks count:', error);
		throw error;
	}
};

export const getCompletedDailyTasksCount = async (): Promise<Number> => {
	try {
		const {
			data: { count },
		} = await $axios.get(`daily-routines/tasks/completed/count`);
		return count;
	} catch (error) {
		console.error('Failed to fetch completed daily tasks count:', error);
		throw error;
	}
};

export const createDailyTask = async (task: Task) => {
	try {
		const {
			data: { data },
		} = await $axios.post('daily-routines/tasks', {
			...task,
			is_daily_routine: true,
		});

		return data;
	} catch (error) {
		console.error('Failed to create daily task:', error);
		throw error;
	}
};

export const updateDailyTask = async (
	taskId: number,
	updates: Partial<Task>,
) => {
	try {
		const data = updates as any;
		const payload: any = {
			title: data.title,
			description: data.description || '',
			status: typeof data.status === 'string' ? data.status : 'backlog',
			project_category_id: data.project_category_id || null,
			order: data.order || null,
			user_id: data.user_id,
			checkpoints: data.checkpoints || [],
			approximately_time: data.approximately_time || null,
			settings: data.settings || null,
			is_daily_routine: true,
			is_recurring: data.is_recurring || false,
			scheduled_date: data.scheduled_date || null,
			scheduled_time: data.scheduled_time || null
		};

		// Only include recurrence if is_recurring is true
		if (data.is_recurring && data.recurrence) {
			payload.recurrence = data.recurrence;
		}

		const {
			data: { data: result },
		} = await $axios.put(`daily-routines/tasks/${taskId}`, payload);

		return result;
	} catch (error) {
		console.error('Failed to update daily task:', error);
		throw error;
	}
};

export const completeDailyTask = async (taskId: number) => {
	try {
		const {
			data: { data },
		} = await $axios.post(`daily-routines/tasks/${taskId}/complete`);

		return data;
	} catch (error) {
		console.error('Failed to complete daily task:', error);
		throw error;
	}
};

export const archiveDailyTask = async (taskId: number) => {
	try {
		const {
			data: { data },
		} = await $axios.post(`daily-routines/tasks/${taskId}/archive`);

		return data;
	} catch (error) {
		console.error('Failed to archive daily task:', error);
		throw error;
	}
};

export const getDailyTask = async (taskId: number) => {
	const {
		data: { data },
	} = await $axios.get(`daily-routines/tasks/${taskId}`);

	return data;
};

export const deleteDailyTask = async (taskId: number) => {
	try {
		const { data } = await $axios.delete(`daily-routines/tasks/${taskId}`);
		return data;
	} catch (error) {
		console.error('Failed to delete daily task:', error);
		throw error;
	}
};

export const deleteDailyTaskInstance = async (taskId: number, taskInstanceId: number) => {
	try {
		const { data } = await $axios.delete(`daily-routines/tasks/${taskId}/instances/${taskInstanceId}`);
		return data;
	} catch (error) {
		console.error('Failed to delete daily task\'s instance:', error);
		throw error;
	}
};

export const completeDailyTaskInstance = async (taskId: number, taskInstanceId: number) => {
	try {
		await $axios.post(`daily-routines/tasks/${taskId}/instances/${taskInstanceId}/complete`);
	} catch (error) {
		console.error('Failed to complete daily task\'s instance:', error);
		throw error;
	}
};
