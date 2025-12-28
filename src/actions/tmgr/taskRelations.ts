import $axios from '@/plugins/axios';

export interface TaskRelationType {
	id: number;
	name: string;
	created_at: string;
	updated_at: string;
}

export interface RelatedTask {
	id: number;
	title: string;
	status_id: number;
	workspace_id: number;
	project_category_id?: number;
}

export interface TaskRelation {
	id: number;
	relation_type: TaskRelationType;
	related_task: RelatedTask;
}

export const getTaskRelationTypes = async (): Promise<TaskRelationType[]> => {
	const { data } = await $axios.get('task-relation-types');
	return data.data;
};

export const searchTasks = async (query: string, limit: number = 10): Promise<RelatedTask[]> => {
	const { data } = await $axios.get('tasks/search', {
		params: { q: query, limit }
	});
	return data;
};

export const addTaskRelation = async (
	taskId: number,
	relatedTaskId: number,
	relationTypeId: number
): Promise<any> => {
	const { data } = await $axios.post(
		`tasks/${taskId}/related-to/${relatedTaskId}/with/${relationTypeId}`
	);
	return data;
};

export const removeTaskRelation = async (
	taskId: number,
	relatedTaskId: number,
	relationTypeId: number
): Promise<any> => {
	const { data } = await $axios.delete(
		`tasks/${taskId}/related-to/${relatedTaskId}/with/${relationTypeId}`
	);
	return data;
};

