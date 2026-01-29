import $axios from '@/plugins/axios';
import { requestCache } from '@/utils/requestCache';

export interface Comment {
	message: string;
}

export const getComments = async (taskId: number, useCache: boolean = false): Promise<Comment[]> => {
	const cacheKey = `comments-task-${taskId}`;
	
	if (useCache) {
		const cached = requestCache.get<Comment[]>(cacheKey);
		if (cached) {
			return cached;
		}
	}

	const {
		data: { data },
	} = await $axios.get(`/tasks/${taskId}/comments/`);

	if (useCache) {
		requestCache.set(cacheKey, data, 30000);
	}

	return data;
};

export const createComment = async (taskId: number, payload: Comment) => {
	const {
		data: { data },
	} = await $axios.post(`/tasks/${taskId}/comments`, payload);

	requestCache.invalidate(`comments-task-${taskId}`);

	return data;
};

export const createAskingHelpComment = async (
	taskId: number,
	question: string,
) => {
	const {
		data: { data },
	} = await $axios.post(`/tasks/${taskId}/comments/help`, { question });

	return data;
};

export const updateComment = async (commentId: number, payload: Comment) => {
	const {
		data: { data },
	} = await $axios.put(`/comments/${commentId}`, payload);

	return data;
};

export const deleteComment = async (commentId: number) => {
	const {
		data: { data },
	} = await $axios.delete(`/comments/${commentId}`);
};
