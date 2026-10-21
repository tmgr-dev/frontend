import $axios from '@/plugins/axios';

export interface TaskTimeEntry {
	id: number;
	user_id: number;
	user_name: string | null;
	source: string;
	started_at: string;
	ended_at: string;
	seconds: number;
	counted_seconds: number;
	created_at: string;
}

export const getTaskTimeEntries = async (
	taskId: number | string,
): Promise<TaskTimeEntry[]> => {
	const {
		data: { data },
	} = await $axios.get(`tasks/${taskId}/time-entries`);
	return Array.isArray(data) ? data : [];
};
