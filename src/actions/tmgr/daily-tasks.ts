import $axios from '@/plugins/axios';
import { Task } from '@/actions/tmgr/tasks';
import type { RoutineEntry, YearStats, IcsImportResult } from '@/types/dailyRoutine';

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
		};
		if ('routine_category' in data) payload.routine_category = data.routine_category;
		if ('scheduled_date' in data) payload.scheduled_date = data.scheduled_date;
		if ('scheduled_time' in data) payload.scheduled_time = data.scheduled_time;

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

// ───── New: calendar virtual expansion ─────

export const expandRoutineRange = async (from: string, to: string): Promise<RoutineEntry[]> => {
	const { data: { data } } = await $axios.get('daily-routines/expand', { params: { from, to } });
	return data;
};

export const expandRoutineYearStats = async (year: number): Promise<YearStats> => {
	const { data: { data } } = await $axios.get('daily-routines/expand/stats', { params: { year } });
	return data;
};

export const completeRoutineOn = async (taskId: number, date: string) => {
	const { data: { data } } = await $axios.post(`daily-routines/tasks/${taskId}/complete-on`, { date });
	return data as { instance_id: number; task_id: number; date: string; status: string; completed: boolean };
};

export const rescheduleRoutineInstance = async (
	taskId: number,
	instanceId: number | 'virtual',
	scheduledFor: string,
) => {
	const { data: { data } } = await $axios.patch(
		`daily-routines/tasks/${taskId}/instances/${instanceId}`,
		{ scheduled_for: scheduledFor }
	);
	return data;
};

export const quickCreateRoutine = async (payload: {
	title: string;
	date?: string;
	time?: string;
	category?: string;
}) => {
	const { data: { data } } = await $axios.post('daily-routines/tasks/quick', payload);
	return data;
};

export const importRoutinesIcs = async (
	file: File,
	mode: 'skip' | 'replace' = 'skip',
): Promise<IcsImportResult> => {
	const fd = new FormData();
	fd.append('file', file);
	fd.append('mode', mode);
	const { data } = await $axios.post('daily-routines/ics/import', fd);
	return data as IcsImportResult;
};

export const exportRoutinesIcsUrl = (ids?: number[]): string => {
	const base = (($axios.defaults.baseURL as string) || '').replace(/\/$/, '');
	const qs = ids && ids.length ? `?ids=${ids.join(',')}` : '';
	return `${base}/daily-routines/ics/export${qs}`;
};

export const downloadRoutinesIcs = async (ids?: number[]): Promise<Blob> => {
	const { data } = await $axios.get('daily-routines/ics/export', {
		params: ids && ids.length ? { ids: ids.join(',') } : {},
		responseType: 'blob',
	});
	return data as Blob;
};
