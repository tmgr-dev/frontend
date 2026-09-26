import type { AxiosInstance } from 'axios';
import type { DataApi } from './broker';
import { encodeFile, MAX_FILE_BYTES } from './fileData';

const unwrap = (response: { data: any }) => response.data?.data ?? null;

/**
 * Plugin data calls go through the app's own axios instance, so they reach the same local workspace
 * adapter as the UI. The header marks the write as the plugin's for domain events.
 */
export const createDataApi = (
	http: AxiosInstance,
	pluginId: string,
): DataApi => {
	const headers = { 'X-TMGR-Plugin': pluginId };
	const storage = (key?: string) =>
		`plugins/${encodeURIComponent(pluginId)}/storage${
			key === undefined ? '' : `/${encodeURIComponent(key)}`
		}`;
	return {
		async listTasks({ statusId, categoryId, search, page, perPage }) {
			const response = await http.get('tasks', {
				headers,
				params: {
					page,
					per_page: perPage,
					...(statusId ? { status_id: statusId } : {}),
					...(categoryId ? { project_category_id: categoryId } : {}),
					...(search ? { search } : {}),
				},
			});
			const items = unwrap(response) ?? [];
			return { items, total: response.data?.meta?.total ?? items.length };
		},
		getTask: async (id) => unwrap(await http.get(`tasks/${id}`, { headers })),
		createTask: async (fields) =>
			unwrap(await http.post('tasks', fields, { headers })),
		updateTask: async (id, fields) =>
			unwrap(await http.patch(`tasks/${id}`, fields, { headers })),
		listStatuses: async () =>
			unwrap(await http.get('workspaces/statuses', { headers })),
		listCategories: async () =>
			unwrap(await http.get('project_categories', { headers })),
		startTimer: async (taskId) =>
			unwrap(
				await http.post(`tasks/${taskId}/countdown`, undefined, { headers }),
			),
		stopTimer: async (taskId) =>
			unwrap(await http.delete(`tasks/${taskId}/countdown`, { headers })),
		listComments: async (taskId) =>
			unwrap(await http.get(`tasks/${taskId}/comments`, { headers })),
		addComment: async (taskId, text) =>
			unwrap(
				await http.post(
					`tasks/${taskId}/comments`,
					{ message: text },
					{ headers },
				),
			),
		storageGet: async (key) =>
			unwrap(await http.get(storage(key), { headers }))?.value ?? null,
		storageSet: async (key, json) =>
			void (await http.put(storage(key), { value: json }, { headers })),
		storageDelete: async (key) =>
			void (await http.delete(storage(key), { headers })),
		storageKeys: async () =>
			unwrap(await http.get(storage(), { headers })) ?? [],
		listAttachments: async (taskId) =>
			(
				(unwrap(await http.get(`tasks/${taskId}/files`, { headers })) ??
					[]) as any[]
			).map((file) => ({
				id: file.id,
				name: file.name,
				mimeType: file.mime_type ?? null,
				size: file.size ?? null,
				createdAt: file.created_at,
			})),
		async readAttachment(fileId) {
			const file = unwrap(await http.get(`files/${fileId}`, { headers }));
			if (Number(file?.size) > MAX_FILE_BYTES)
				throw new Error('the file is larger than 5 MB');
			const content = (
				await http.get(`files/${fileId}/content`, {
					headers,
					responseType: 'blob',
				})
			).data as Blob;
			const bytes = new Uint8Array(await content.arrayBuffer());
			return {
				name: file.name,
				mimeType: file.mime_type ?? null,
				...encodeFile(bytes, file.mime_type ?? null, file.name),
			};
		},
	};
};
