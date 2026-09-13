import $axios from '@/plugins/axios';
import {
	absoluteLinkUrl,
	createSignedLinkCache,
} from '@/utils/signedFileLinks';
import { withThumbParam } from '@/utils/thumbnailUrl';

export interface TaskFile {
	id: number;
	task_id: number;
	user_id: number | null;
	workspace_id: number | null;
	name: string;
	original_name: string | null;
	file_path: string;
	mime_type: string | null;
	size: number | null;
	created_at: string;
}

export interface PresignedUpload {
	key: string;
	upload_url: string;
	method: string;
	/** The exact type the URL was signed for; the PUT must send this, not the browser's guess. */
	content_type: string;
	max_bytes: number;
}

export const getTaskFiles = async (taskId: number): Promise<TaskFile[]> => {
	const {
		data: { data },
	} = await $axios.get(`/tasks/${taskId}/files`);

	return data;
};

export const presignUpload = async (file: File): Promise<PresignedUpload> => {
	const {
		data: { data },
	} = await $axios.post('/files/presign-upload', {
		file_name: file.name,
		content_type: file.type,
		size_bytes: file.size,
	});

	return data;
};

/**
 * Uploads straight to object storage. Deliberately `fetch` and not the axios instance: the presigned
 * URL carries its own credentials in the query string, and storage refuses a request that also
 * carries an Authorization header. The signature covers the content type and the exact length, so
 * the type has to be the one the API echoed back and the body has to be the file itself.
 */
export const putToStorage = async (
	target: PresignedUpload,
	file: File,
): Promise<void> => {
	const response = await fetch(target.upload_url, {
		method: 'PUT',
		headers: { 'Content-Type': target.content_type },
		body: file,
	});

	if (!response.ok) {
		throw new Error(`storage responded ${response.status}`);
	}
};

export const attachFile = async (
	taskId: number,
	file: File,
	target: PresignedUpload,
): Promise<TaskFile> => {
	const {
		data: { data },
	} = await $axios.post(`/tasks/${taskId}/files`, {
		file_name: file.name,
		file_path: target.key,
		mime_type: target.content_type,
		size_bytes: file.size,
	});

	return data;
};

/** presign → PUT → attach. Rejects with the axios error of whichever step failed. */
export const uploadTaskFile = async (
	taskId: number,
	file: File,
): Promise<TaskFile> => {
	const target = await presignUpload(file);
	await putToStorage(target, file);

	return attachFile(taskId, file, target);
};

export const detachFile = async (fileId: number): Promise<void> => {
	await $axios.delete(`/files/${fileId}`);
};

/**
 * Pulls the bytes through the API and wraps them in an object URL. Still how a download is
 * started - a browser ignores `download` on a cross-origin href - and the fallback for a
 * deployment that does not sign links. The caller owns the returned URL and must revoke it.
 */
export const fetchFileObjectUrl = async (
	fileId: number,
	options: { thumb?: boolean } = {},
): Promise<string> => {
	const { data } = await $axios.get(`/files/${fileId}/content`, {
		responseType: 'blob',
		params: options.thumb ? { thumb: true } : undefined,
	});

	return URL.createObjectURL(data);
};

const signedLinks = createSignedLinkCache({
	sign: async (fileId: number) => {
		try {
			const {
				data: { data },
			} = await $axios.get(`/files/${fileId}/signed-url`);

			return {
				url: absoluteLinkUrl(data.url, import.meta.env.VITE_API_BASE_URL),
				expiresAt: Date.parse(data.expires_at),
			};
		} catch (error) {
			// 503 means this deployment has no signing secret; anything else is the caller's problem.
			if (
				(error as { response?: { status?: number } })?.response?.status === 503
			) {
				return null;
			}

			throw error;
		}
	},
});

/**
 * A URL an `<img>` can load: a signed link where the deployment signs them, an object URL
 * otherwise. Pair every call with `releaseFileDisplayUrl` - only the fallback needs revoking.
 *
 * `thumb` asks for the small rendering, which is what a list wants: without it a page of tiles
 * downloads every image at full size to draw them at 48 pixels.
 */
export const fileDisplayUrl = async (
	fileId: number,
	options: { thumb?: boolean } = {},
): Promise<string> => {
	const signed = await signedLinks.get(fileId);
	if (signed) {
		return options.thumb ? withThumbParam(signed) : signed;
	}

	return fetchFileObjectUrl(fileId, options);
};

export const releaseFileDisplayUrl = (url: string | null | undefined): void => {
	if (url?.startsWith('blob:')) {
		URL.revokeObjectURL(url);
	}
};

export interface WorkspaceFile {
	id: number;
	name: string;
	mime_type: string | null;
	size: number | null;
	user_id: number | null;
	created_at: string;
	task: { id: number; key: string | null; title: string | null };
}

export interface WorkspaceFilePage {
	data: WorkspaceFile[];
	meta: {
		current_page: number;
		per_page: number;
		total: number;
		last_page: number;
	};
}

/** One page of everything attached anywhere in the workspace, each row naming its task. */
export const getWorkspaceFiles = async (
	workspaceId: number,
	options: { page?: number; perPage?: number; images?: boolean } = {},
): Promise<WorkspaceFilePage> => {
	const { data } = await $axios.get(`/workspaces/${workspaceId}/files`, {
		params: {
			page: options.page ?? 1,
			per_page: options.perPage ?? 40,
			images: options.images ?? false,
		},
	});

	return data;
};
