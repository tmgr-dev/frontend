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

export interface NewTimeEntry {
	client_interval_id: string;
	source: 'activity';
	started_at: string;
	ended_at: string;
	seconds: number;
}

export interface TimeEntryResult {
	client_interval_id?: string;
	status: 'created' | 'duplicate' | 'conflict' | string;
}

export const createTimeEntries = async (
	taskId: number | string,
	entries: NewTimeEntry[],
): Promise<TimeEntryResult[]> => {
	const { data } = await $axios.post(`tasks/${taskId}/time-entries`, {
		entries,
	});
	const results = data?.entries ?? data?.data?.entries;
	return Array.isArray(results) ? results : [];
};
