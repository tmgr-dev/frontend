import $axios from '@/plugins/axios';
import { requestCache } from '@/utils/requestCache';

export interface TaskFile {
	id: number;
	name: string;
	original_name: string | null;
	file_path: string;
	mime_type: string | null;
	size: number | null;
	task_id: number;
	user_id: number;
	workspace_id: number;
	created_at: string | null;
}

export const getFileType = (mimeType: string | null, fileName: string): 'image' | 'video' | 'pdf' | 'text' | 'other' => {
	const ext = fileName.split('.').pop()?.toLowerCase() || '';
	
	if (ext === 'pdf' || mimeType === 'application/pdf') return 'pdf';
	
	if (mimeType) {
		if (mimeType.startsWith('image/')) return 'image';
		if (mimeType.startsWith('video/')) return 'video';
		if (mimeType.startsWith('text/') || mimeType === 'application/json') return 'text';
	}
	
	if (['jpg', 'jpeg', 'png', 'gif', 'webp', 'svg', 'bmp'].includes(ext)) return 'image';
	if (['mp4', 'webm', 'ogg', 'mov', 'avi', 'mkv'].includes(ext)) return 'video';
	if (['txt', 'md', 'json', 'js', 'ts', 'css', 'html', 'xml', 'yaml', 'yml', 'log', 'csv'].includes(ext)) return 'text';
	
	return 'other';
};

export const isPreviewable = (mimeType: string | null, fileName: string): boolean => {
	const type = getFileType(mimeType, fileName);
	return type !== 'other';
};

export const getTaskFiles = async (
	taskId: number,
	useCache: boolean = false,
): Promise<TaskFile[]> => {
	const cacheKey = `files-task-${taskId}`;

	if (useCache) {
		const cached = requestCache.get<TaskFile[]>(cacheKey);
		if (cached) {
			return cached;
		}
	}

	const {
		data: { data },
	} = await $axios.get(`/tasks/${taskId}/files`);

	if (useCache) {
		requestCache.set(cacheKey, data, 30000);
	}

	return data;
};

export const uploadTaskFile = async (
	taskId: number,
	file: File,
): Promise<TaskFile> => {
	const formData = new FormData();
	formData.append('file', file);
	formData.append('name', file.name);

	const { data } = await $axios.post(`/tasks/${taskId}/files`, formData, {
		headers: {
			'Content-Type': 'multipart/form-data',
		},
	});

	requestCache.invalidate(`files-task-${taskId}`);

	return data;
};

export const deleteTaskFile = async (
	fileId: number,
	taskId?: number,
): Promise<void> => {
	await $axios.delete(`/files/${fileId}`);

	if (taskId) {
		requestCache.invalidate(`files-task-${taskId}`);
	}
};

export const downloadTaskFile = async (fileId: number, fileName: string): Promise<void> => {
	const response = await $axios.get(`/files/${fileId}?download=1`, {
		responseType: 'blob',
	});

	const blob = response.data instanceof Blob ? response.data : new Blob([response.data]);
	const url = window.URL.createObjectURL(blob);
	const link = document.createElement('a');
	link.href = url;
	link.setAttribute('download', fileName);
	document.body.appendChild(link);
	link.click();
	link.remove();
	window.URL.revokeObjectURL(url);
};

export const getFilePreviewUrl = async (fileId: number, mimeType?: string | null): Promise<string> => {
	const response = await $axios.get(`/files/${fileId}`, {
		responseType: 'blob',
	});

	const blob = response.data instanceof Blob 
		? response.data 
		: new Blob([response.data], mimeType ? { type: mimeType } : undefined);
	
	return window.URL.createObjectURL(blob);
};

export const getFileTextContent = async (fileId: number): Promise<string> => {
	const response = await $axios.get(`/files/${fileId}`, {
		responseType: 'text',
	});

	return response.data;
};
