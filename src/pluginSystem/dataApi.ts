import type { AxiosInstance } from 'axios';
import { PluginError, type DataApi } from './broker';
import { encodeFile, MAX_FILE_BYTES } from './fileData';

const unwrap = (response: { data: any }) => response.data?.data ?? null;

const withAuthor = (comment: any) =>
	comment && typeof comment === 'object' && !comment.author
		? {
				...comment,
				author: {
					kind: 'user',
					id: String(comment.user_id ?? comment.userId ?? ''),
					name: '',
				},
		  }
		: comment;

/**
 * Plugin data calls go through a client pinned to the plugin's workspace (see src/local/pinned.ts).
 * The headers mark the write as the plugin's for domain events and comment/reaction authorship.
 */
/** `storageId` namespaces the plugin's storage; installed plugins include their repository in it. */
export const createDataApi = (
	http: AxiosInstance,
	pluginId: string,
	storageId = pluginId,
	pluginName = pluginId,
	cloud = false,
): DataApi => {
	// Header values must be Latin-1; plugin names may not be.
	const headers = {
		'X-TMGR-Plugin': pluginId,
		'X-TMGR-Plugin-Name': encodeURIComponent(pluginName),
	};
	const notInSharedWorkspaces = (what: string) => {
		if (cloud)
			throw new PluginError(
				'NOT_SUPPORTED',
				`${what} is not available in shared workspaces yet`,
			);
	};
	const storage = (key?: string) =>
		`plugins/${encodeURIComponent(storageId)}/storage${
			key === undefined ? '' : `/${encodeURIComponent(key)}`
		}`;
	let relationTypes: Promise<Map<string, number>> | null = null;
	const relationTypeId = async (name: string): Promise<number> => {
		if (!relationTypes) {
			relationTypes = http
				.get('task-relation-types', { headers })
				.then((response) => {
					const rows = (unwrap(response) ?? []) as {
						id: number;
						name: string;
					}[];
					return new Map(rows.map((row) => [row.name, row.id]));
				});
		}
		const found = (await relationTypes).get(name);
		if (found === undefined) throw new Error(`unknown relation type ${name}`);
		return found;
	};
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
			(
				(unwrap(await http.get(`tasks/${taskId}/comments`, { headers })) ??
					[]) as any[]
			).map(withAuthor),
		addComment: async (taskId, text) =>
			withAuthor(
				unwrap(
					await http.post(
						`tasks/${taskId}/comments`,
						{ message: text },
						{ headers },
					),
				),
			),
		reactToComment: async (commentId, emoji) => {
			const body = unwrap(
				await http.post(
					`comments/${commentId}/reactions/toggle`,
					{ emoji },
					{ headers },
				),
			);
			const reactions = Array.isArray(body)
				? body
				: Array.isArray((body as any)?.reactions)
				? (body as any).reactions
				: [];
			const taskId = Array.isArray(body)
				? undefined
				: (body as any)?.task_id ?? (body as any)?.taskId;
			return {
				reactions,
				...(taskId != null ? { taskId: Number(taskId) } : {}),
			};
		},
		listRelations: async (taskId) => {
			notInSharedWorkspaces('reading task relations');
			return (
				(unwrap(await http.get(`tasks/${taskId}/relations`, { headers })) ??
					[]) as any[]
			).map((row: any) => ({
				taskId,
				otherTaskId: row.related_task?.id,
				type: row.relation_type?.name,
			}));
		},
		relateTask: async (taskId, otherId, type) => {
			const typeId = await relationTypeId(type);
			return unwrap(
				await http.post(
					`tasks/${taskId}/related-to/${otherId}/with/${typeId}`,
					undefined,
					{
						headers,
					},
				),
			);
		},
		unrelateTask: async (taskId, otherId, type) => {
			const typeId = await relationTypeId(type);
			return unwrap(
				await http.delete(
					`tasks/${taskId}/related-to/${otherId}/with/${typeId}`,
					{ headers },
				),
			);
		},
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
