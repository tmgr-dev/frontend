import $axios from '@/plugins/axios';
import { AxiosRequestConfig } from 'axios';
import { Task } from '@/actions/tmgr/tasks';

export const generateLink = async (): Promise<Task[]> => {
	try {
		const {
			data: { link },
		} = await $axios.post(`telegram/link/generate`);
		return link;
	} catch (error) {
		console.error('Failed to fetch daily tasks:', error);
		throw error;
	}
};

export const unlink = async (): Promise<void> => {
	try {
		await $axios.post(`telegram/unlink`);
	} catch (error) {
		console.error('Failed to fetch daily tasks:', error);
		throw error;
	}
};
