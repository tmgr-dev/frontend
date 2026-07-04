import { api } from './client';

export type Comment = {
	id: number;
	message: string;
	user: { id: number; name: string; email?: string; avatar?: string };
	created_at: string;
	updated_at?: string;
	reactions?: any[];
	cursor_agent_id?: number | null;
};

export async function getComments(taskId: number): Promise<Comment[]> {
	const { data } = await api.get(`tasks/${taskId}/comments`);
	return data?.data ?? data ?? [];
}

export async function createComment(
	taskId: number,
	message: string,
): Promise<Comment> {
	const { data } = await api.post(`tasks/${taskId}/comments`, { message });
	return data?.data ?? data;
}

export async function deleteComment(commentId: number): Promise<void> {
	await api.delete(`comments/${commentId}`);
}

export async function askAIComment(
	taskId: number,
	question: string,
): Promise<any> {
	const { data } = await api.post(`tasks/${taskId}/comments/help`, { question });
	return data?.data ?? data;
}

// NOTE: reaction endpoint not confirmed against the web action layer yet.
// Best-effort; verify/adjust once the backend route is confirmed.
export async function toggleReaction(
	commentId: number,
	emoji: string,
): Promise<any> {
	const { data } = await api.post(`comments/${commentId}/reactions`, { emoji });
	return data?.data ?? data;
}
