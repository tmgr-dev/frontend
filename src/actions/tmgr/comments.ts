import $axios from '@/plugins/axios';
import type { ReactionSummary } from '@/utils/commentReactions';
import {
	extractReactionsPayload,
	normalizeReactions,
} from '@/utils/commentReactions';
import { requestCache } from '@/utils/requestCache';

export interface Comment {
	message: string;
}

export const getComments = async (
	taskId: number,
	useCache = false,
): Promise<Comment[]> =>
	requestCache.getOrFetch(
		`comments-task-${taskId}`,
		async () => {
			const {
				data: { data },
			} = await $axios.get(`/tasks/${taskId}/comments/`);
			return data;
		},
		{ ttl: 30000, cache: useCache },
	);

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

export const toggleCommentReaction = async (
	commentId: number,
	emoji: string,
	currentUserId?: number,
): Promise<ReactionSummary[]> => {
	const response = await $axios.post(
		`/comments/${commentId}/reactions/toggle`,
		{ emoji },
	);

	return normalizeReactions(
		extractReactionsPayload(response.data),
		currentUserId,
	);
};
