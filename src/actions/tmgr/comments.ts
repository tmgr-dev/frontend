import $axios from 'src/plugins/axios';

export interface Comment {
	message: string;
}

export const getComments = async (taskId: number): Promise<Comment[]> => {
	const {
		data: { data },
	} = await $axios.get(`/tasks/${taskId}/comments/`);

	return data;
};

export const createComment = async (taskId: number, payload: Comment) => {
	const {
		data: { data },
	} = await $axios.post(`/tasks/${taskId}/comments`, payload);

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
