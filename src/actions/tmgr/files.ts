import $axios from '@/plugins/axios';
import { requestCache } from '@/utils/requestCache';
import downloadFile from '@/utils/downloadFile';

// Task attachments are served by the Java backend's files module (S3-backed),
// which is snake_case like the rest of the app's API, confirmed live against
// stage — an earlier draft of this module assumed camelCase (it isn't).
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

interface PresignedUpload {
	key: string;
	upload_url: string;
	method: string;
}

const presignUpload = async (fileName: string, contentType: string): Promise<PresignedUpload> => {
	const {
		data: { data },
	} = await $axios.post('/files/presign-upload', {
		file_name: fileName,
		content_type: contentType,
	});

	return data;
};

const presignDownload = async (key: string): Promise<string> => {
	const {
		data: { data },
	} = await $axios.post('/files/presign-download', { key });

	return data.download_url;
};

// Uploads go straight to S3 via a presigned URL, not through our backend —
// $axios always attaches a bearer token and a 30s timeout meant for API
// calls, neither of which belongs on a direct-to-S3 PUT of arbitrary file
// size, so this uses a bare fetch. S3 also requires the Content-Type on the
// PUT to match the one used to sign the URL, or the request is rejected.
export const uploadTaskFile = async (
	taskId: number,
	file: File,
): Promise<TaskFile> => {
	const contentType = file.type || 'application/octet-stream';
	const { key, upload_url, method } = await presignUpload(file.name, contentType);

	const putResponse = await fetch(upload_url, {
		method,
		headers: { 'Content-Type': contentType },
		body: file,
	});
	if (!putResponse.ok) {
		throw new Error(`S3 upload failed: ${putResponse.status}`);
	}

	const {
		data: { data },
	} = await $axios.post(`/tasks/${taskId}/files`, {
		file_name: file.name,
		file_path: key,
		mime_type: contentType,
		size_bytes: file.size,
	});

	requestCache.invalidate(`files-task-${taskId}`);

	return data;
};

export const deleteTaskFile = async (
	fileId: number,
	taskId: number,
): Promise<void> => {
	await $axios.delete(`/files/${fileId}`);

	requestCache.invalidate(`files-task-${taskId}`);
};

// Exported (not just an internal helper) so callers that need to derive more
// than one independent object URL from the same file (e.g. a grid thumbnail
// and, later, a preview modal for that same file) can do so from one fetch
// instead of sharing a single URL string across two owners with different
// lifetimes.
export const getFileBlob = async (file: Pick<TaskFile, 'id' | 'file_path'>): Promise<Blob> => {
	const url = await presignDownload(file.file_path);
	const response = await fetch(url);
	if (!response.ok) {
		throw new Error(`Presigned download failed: ${response.status}`);
	}

	return response.blob();
};

export const downloadTaskFile = async (file: Pick<TaskFile, 'id' | 'file_path'>, fileName: string): Promise<void> => {
	const blob = await getFileBlob(file);
	downloadFile(blob, fileName);
};

export const getFilePreviewUrl = async (file: Pick<TaskFile, 'id' | 'file_path'>): Promise<string> => {
	const blob = await getFileBlob(file);

	return window.URL.createObjectURL(blob);
};

export const getFileTextContent = async (file: Pick<TaskFile, 'id' | 'file_path'>): Promise<string> => {
	const blob = await getFileBlob(file);

	return blob.text();
};
