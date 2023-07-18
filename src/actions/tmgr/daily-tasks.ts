import $axios from 'src/plugins/axios';
import { AxiosRequestConfig } from 'axios';
import { Task } from 'src/actions/tmgr/tasks';

export const getDailyTasks = async (): Promise<Task[]> => {
	const {
		data: { data },
	} = await $axios.get(`daily-routines/tasks`);

	return data;
};

export const createDailyTask = async (task: Task) => {
	const {
		data: { data },
	} = await $axios.post('daily-routines/tasks', task);

	return data;
};
