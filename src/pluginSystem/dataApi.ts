import type { AxiosInstance } from 'axios';
import { PluginError, type DataApi } from './broker';
import { encodeFile, MAX_FILE_BYTES } from './fileData';

const unwrap = (response: { data: any }) => response.data?.data ?? null;

/** Until the server names a plugin token's comment as its own (TM-296): unknown author is unknown, not 'user'. */
const withUnknownAuthor = (comment: any) =>
	comment && typeof comment === 'object' && !comment.author
		? { ...comment, author: null }
		: comment;

/** A comment this call just created under this plugin's token: we know it is ours even if the server doesn't say so. */
const withOwnAuthor = (comment: any, pluginId: string, pluginName: string) =>
	comment && typeof comment === 'object' && !comment.author
		? { ...comment, author: { kind: 'plugin', id: pluginId, name: pluginName } }
		: comment;

/** The 7 relation types are fixed and identical on the server and in the local workspace. */
const RELATION_TYPE_IDS: Record<string, number> = {
	blocks: 1,
	'is blocked by': 2,
	'relates to': 3,
	duplicates: 4,
	'is duplicated by': 5,
	'depends on': 6,
	'is dependency of': 7,
};

const relationTypeId = (name: string): number => {
	const found = RELATION_TYPE_IDS[name];
	if (found === undefined) throw new Error(`unknown relation type ${name}`);
	return found;
};

/**
 * Plugin data calls go through a client pinned to the plugin's workspace (see src/local/pinned.ts).
 * The headers mark the write as the plugin's for domain events and comment/reaction authorship.
 */
/** `storageId` namespaces the plugin's storage; installed plugins include their repository in it. */
/** `key: "CODE-N"` from the category's code and this task's ticket number in it, else null. */
export const taskKey = (task: any): string | null =>
	task?.category?.code && task?.category_tasks_sequence_id != null
		? `${task.category.code}-${task.category_tasks_sequence_id}`
		: null;

const withKey = (task: any) =>
	task && typeof task === 'object' ? { ...task, key: taskKey(task) } : task;

export const createDataApi = (
	http: AxiosInstance,
	pluginId: string,
	storageId = pluginId,
	pluginName = pluginId,
	cloud = false,
	workspaceId?: number,
): DataApi => {
	// Header values must be Latin-1; plugin names may not be. Storage id is set here, and only here: it is
	// what pinned.ts (local) reads to tell apart repositories that reuse the same plugin id.
	const headers: Record<string, string> = {
		'X-TMGR-Plugin': pluginId,
		'X-TMGR-Plugin-Name': encodeURIComponent(pluginName),
		...(cloud ? {} : { 'X-TMGR-Plugin-Storage': encodeURIComponent(storageId) }),
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
	return {
		async listTasks({
			statusId,
			categoryId,
			search,
			page,
			perPage,
			updatedSince,
			dueBefore,
			dueAfter,
			statusType,
			priority,
			sort,
			direction,
		}) {
			if (updatedSince || dueBefore || dueAfter || statusType || priority || sort) {
				notInSharedWorkspaces('this task filter or sort');
			}
			const response = await http.get('tasks', {
				headers,
				params: {
					page,
					per_page: perPage,
					...(statusId ? { status_id: statusId } : {}),
					...(categoryId ? { project_category_id: categoryId } : {}),
					...(search ? { search } : {}),
					...(updatedSince ? { updated_since: updatedSince } : {}),
					...(dueBefore ? { due_before: dueBefore } : {}),
					...(dueAfter ? { due_after: dueAfter } : {}),
					...(statusType ? { status_type: statusType } : {}),
					...(priority ? { priority } : {}),
					...(sort ? { sort, direction: direction ?? 'asc' } : {}),
				},
			});
			const items = ((unwrap(response) ?? []) as any[]).map(withKey);
			return { items, total: response.data?.meta?.total ?? items.length };
		},
		getTask: async (id) => withKey(unwrap(await http.get(`tasks/${id}`, { headers }))),
		createTask: async (fields) =>
			withKey(unwrap(await http.post('tasks', fields, { headers }))),
		updateTask: async (id, fields) => {
			// The server ignores a null expired_at instead of clearing it; NOT_SUPPORTED beats a silent no-op.
			if (cloud && 'expired_at' in fields && fields.expired_at === null) {
				notInSharedWorkspaces('clearing the due date');
			}
			return withKey(unwrap(await http.patch(`tasks/${id}`, fields, { headers })));
		},
		listStatuses: async () =>
			unwrap(await http.get('workspaces/statuses', { headers })),
		listCategories: async () =>
			unwrap(await http.get('project_categories', { headers })),
		createStatus: async (fields) =>
			unwrap(await http.post(`workspaces/${workspaceId}/statuses`, fields, { headers })),
		updateStatus: async (id, patch) => {
			if (cloud && 'type' in patch) notInSharedWorkspaces('changing a status type');
			return unwrap(await http.put(`statuses/${id}`, patch, { headers }));
		},
		reorderStatuses: async (ids) =>
			unwrap(
				await http.put(
					`workspaces/${workspaceId}/statuses/order`,
					{
						statuses_with_order: ids.map((statusId, index) => ({
							status_id: statusId,
							order: index + 1,
						})),
					},
					{ headers },
				),
			),
		createCategory: async (fields) =>
			unwrap(
				await http.post(
					'project_categories',
					cloud ? { ...fields, workspace_id: workspaceId } : fields,
					{ headers },
				),
			),
		updateCategory: async (id, patch) => {
			if (cloud && 'code' in patch) notInSharedWorkspaces('changing a category code');
			return unwrap(await http.put(`project_categories/${id}`, patch, { headers }));
		},
		startTimer: async (taskId) =>
			withKey(
				unwrap(
					await http.post(`tasks/${taskId}/countdown`, undefined, { headers }),
				),
			),
		stopTimer: async (taskId) =>
			withKey(unwrap(await http.delete(`tasks/${taskId}/countdown`, { headers }))),
		listComments: async (taskId) =>
			(
				(unwrap(await http.get(`tasks/${taskId}/comments`, { headers })) ??
					[]) as any[]
			).map(withUnknownAuthor),
		addComment: async (taskId, text) =>
			withOwnAuthor(
				unwrap(
					await http.post(
						`tasks/${taskId}/comments`,
						{ message: text },
						{ headers },
					),
				),
				pluginId,
				pluginName,
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
		taskDataGet: async (taskId, key) => {
			notInSharedWorkspaces('per-task plugin data');
			return (
				unwrap(
					await http.get(
						`plugins/${encodeURIComponent(storageId)}/tasks/${taskId}/data/${encodeURIComponent(key)}`,
						{ headers },
					),
				)?.value ?? null
			);
		},
		taskDataSet: async (taskId, key, json) => {
			notInSharedWorkspaces('per-task plugin data');
			void (await http.put(
				`plugins/${encodeURIComponent(storageId)}/tasks/${taskId}/data/${encodeURIComponent(key)}`,
				{ value: json },
				{ headers },
			));
		},
		taskDataDelete: async (taskId, key) => {
			notInSharedWorkspaces('per-task plugin data');
			void (await http.delete(
				`plugins/${encodeURIComponent(storageId)}/tasks/${taskId}/data/${encodeURIComponent(key)}`,
				{ headers },
			));
		},
		taskDataGetMany: async (taskIds, key) => {
			notInSharedWorkspaces('per-task plugin data');
			return (
				unwrap(
					await http.post(
						`plugins/${encodeURIComponent(storageId)}/task-data/query`,
						{ task_ids: taskIds, key },
						{ headers },
					),
				) ?? {}
			);
		},
		listAgentWork: async (taskId) =>
			unwrap(await http.get(`tasks/${taskId}/agent-work`, { headers })),
		startAgentWork: async (taskId, fields) =>
			unwrap(
				await http.post(
					`tasks/${taskId}/agent-work`,
					{
						// Namespaced under this plugin's own identity: it can never claim to be a bare agent.
						agent: `plugin:${pluginId}${fields.agent ? `/${fields.agent}` : ''}`,
						model: fields.model,
						session_id: fields.sessionId,
						branch: fields.branch,
					},
					{ headers },
				),
			),
		updateAgentWork: async (runId, patch) =>
			unwrap(
				await http.patch(
					`agent-work/${runId}`,
					{
						...('branch' in patch ? { branch: patch.branch } : {}),
						...('summary' in patch ? { summary: patch.summary } : {}),
						...('prUrl' in patch ? { pr_url: patch.prUrl } : {}),
						...('commits' in patch ? { commits: patch.commits } : {}),
						...('tests' in patch ? { tests: patch.tests } : {}),
					},
					{ headers },
				),
			),
		finishAgentWork: async (runId, patch) =>
			unwrap(
				await http.post(
					`agent-work/${runId}/finish`,
					{
						status: patch.status,
						...('summary' in patch ? { summary: patch.summary } : {}),
						...('prUrl' in patch ? { pr_url: patch.prUrl } : {}),
						...('commits' in patch ? { commits: patch.commits } : {}),
						...('tests' in patch ? { tests: patch.tests } : {}),
					},
					{ headers },
				),
			),
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
