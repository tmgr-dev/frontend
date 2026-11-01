import { CloudError } from './batch';

export { CloudError };

export interface Http {
	get(url: string, config?: any): Promise<{ data: any }>;
	post(url: string, body?: unknown, config?: any): Promise<{ data: any }>;
}

export interface CloudStatus {
	id: number;
	name?: string;
	pivot?: { is_active?: number | boolean; order?: number };
}

export interface CloudCategory {
	id: number;
	title: string;
}

export interface CloudApi {
	statuses(workspaceId: number): Promise<CloudStatus[]>;
	categories(workspaceId: number): Promise<CloudCategory[]>;
	createRoutine(
		path: string,
		body: Record<string, any>,
	): Promise<{ id: number | null }>;
	createTask(
		body: Record<string, any>,
		key: string,
	): Promise<{ id: number | null }>;
}

export const toCloudError = (error: unknown): CloudError => {
	if (error instanceof CloudError) return error;
	const response = (error as any)?.response;
	if (!response)
		return new CloudError(
			'network',
			(error as Error)?.message || 'The TMGR server could not be reached.',
		);
	if (response.status === 401)
		return new CloudError(
			'auth',
			response.data?.message || 'Your session expired.',
		);
	return new CloudError(
		'rejected',
		response.data?.message ||
			response.data?.error ||
			`The server answered ${response.status}.`,
	);
};

export const firstActiveStatusId = (statuses: CloudStatus[]): number | null => {
	const active = statuses
		.filter((s) => s.pivot?.is_active)
		.sort(
			(a, b) => (a.pivot?.order ?? 0) - (b.pivot?.order ?? 0) || a.id - b.id,
		);
	return active[0]?.id ?? null;
};

export const createCloudApi = (
	http: Http,
	options: { refreshSession?: () => Promise<void> } = {},
): CloudApi => {
	const send = async <T>(call: () => Promise<T>): Promise<T> => {
		try {
			return await call();
		} catch (first) {
			const failure = toCloudError(first);
			if (failure.kind !== 'auth' || !options.refreshSession) throw failure;
			try {
				await options.refreshSession();
				return await call();
			} catch (second) {
				throw toCloudError(second);
			}
		}
	};
	const headers = (
		workspaceId: number,
		extra: Record<string, string> = {},
	) => ({
		headers: { 'X-Workspace-Id': String(workspaceId), ...extra },
	});
	const created = (response: { data: any }) => {
		const id = response.data?.data?.id ?? response.data?.id;
		if (id == null)
			throw new CloudError(
				'rejected',
				'The server did not confirm the routine was created.',
			);
		return { id };
	};
	return {
		statuses: (workspaceId) =>
			send(
				async () =>
					(
						await http.get(
							`workspaces/${workspaceId}/statuses`,
							headers(workspaceId),
						)
					).data?.data ?? [],
			),
		categories: (workspaceId) =>
			send(
				async () =>
					(
						await http.get(
							`project_categories?all&workspace_id=${workspaceId}`,
							headers(workspaceId),
						)
					).data?.data ?? [],
			),
		createRoutine: (path, body) =>
			send(async () =>
				created(await http.post(path, body, headers(body.workspace_id))),
			),
		createTask: (body, key) =>
			send(async () =>
				created(
					await http.post(
						'tasks',
						body,
						headers(body.workspace_id, { 'Idempotency-Key': key }),
					),
				),
			),
	};
};
