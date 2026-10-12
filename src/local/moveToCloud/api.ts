export interface ImportCapabilities {
	bundle_schemas: number[];
	local_schemas: number[];
	mime_allowlist: string[];
	max_file_bytes: number;
	max_bundle_bytes: number;
	max_expanded_bytes: number;
	max_attachment_bytes: number;
	jobs_per_day: number;
}

export type PersonaAction = 'create' | 'map' | 'me';

export interface PersonaMappingEntry {
	local_uuid: string;
	action: PersonaAction;
	cloud_uuid?: string;
}

export interface CreateJobRequest {
	fingerprint: string;
	name: string;
	counts: Record<string, number>;
	bytes: number;
	bundle_schema: number;
	local_schema: number;
	source_revision: string;
	personas: PersonaMappingEntry[];
}

export interface CreatedJob {
	job_id: string;
	upload_url: string;
	upload_headers: Record<string, string>;
	status: string;
}

export interface FilePresignRequest {
	sha256: string;
	size: number;
	mime: string;
}

export interface PresignedUpload {
	upload_url: string;
	upload_headers: Record<string, string>;
}

export type ImportStatusName =
	| 'created'
	| 'uploaded'
	| 'running'
	| 'failed'
	| 'verified'
	| 'published'
	| 'discarded';

export interface ImportStatus {
	status: ImportStatusName;
	phase: string | null;
	done: number;
	total: number;
	errors: unknown[];
	report: Record<string, any> | null;
	workspace_id?: number | null;
}

export interface Http {
	get<T = any>(url: string): Promise<{ data: T }>;
	post<T = any>(url: string, body?: unknown): Promise<{ data: T }>;
	delete<T = any>(url: string): Promise<{ data: T }>;
}

export const UNSUPPORTED_SCHEMA_MESSAGE = 'Update TMGR to move this workspace.';

export interface ImportApi {
	capabilities(): Promise<ImportCapabilities>;
	createJob(request: CreateJobRequest): Promise<CreatedJob>;
	bundleUploaded(jobId: string): Promise<void>;
	presignFile(
		jobId: string,
		file: FilePresignRequest,
	): Promise<PresignedUpload>;
	confirmFile(jobId: string, sha256: string): Promise<void>;
	commit(jobId: string): Promise<void>;
	status(jobId: string): Promise<ImportStatus>;
	retry(jobId: string): Promise<void>;
	publish(
		jobId: string,
		acceptDifferences?: boolean,
	): Promise<{ workspace_id?: number | null }>;
	discard(jobId: string): Promise<void>;
}

const BASE = '/imports';

export const createImportApi = (http: Http): ImportApi => {
	const job = (id: string) => `${BASE}/${encodeURIComponent(id)}`;
	return {
		capabilities: async () =>
			(
				await http.get<ImportCapabilities>(
					`${BASE}/local-workspace/capabilities`,
				)
			).data,
		createJob: async (request) =>
			(await http.post<CreatedJob>(`${BASE}/local-workspace`, request)).data,
		bundleUploaded: async (id) => {
			await http.post(`${job(id)}/bundle/uploaded`);
		},
		presignFile: async (id, file) =>
			(await http.post<PresignedUpload>(`${job(id)}/files/presign`, file)).data,
		confirmFile: async (id, sha256) => {
			await http.post(`${job(id)}/files/confirm`, { sha256 });
		},
		commit: async (id) => {
			await http.post(`${job(id)}/commit`);
		},
		status: async (id) => (await http.get<ImportStatus>(job(id))).data,
		retry: async (id) => {
			await http.post(`${job(id)}/retry`);
		},
		publish: async (id, acceptDifferences = false) =>
			(
				await http.post(
					`${job(id)}/publish`,
					acceptDifferences ? { accept_differences: true } : {},
				)
			).data ?? {},
		discard: async (id) => {
			await http.delete(job(id));
		},
	};
};

export const isUnsupportedSchema = (error: unknown): boolean => {
	const response = (error as any)?.response;
	return (
		response?.status === 422 &&
		response?.data?.error === 'unsupported_bundle_schema'
	);
};

export const apiErrorMessage = (error: unknown): string => {
	if (isUnsupportedSchema(error)) return UNSUPPORTED_SCHEMA_MESSAGE;
	const data = (error as any)?.response?.data;
	return (
		data?.message ||
		(error as any)?.message ||
		String(error) ||
		'Something went wrong.'
	);
};

let shared: ImportApi | null = null;

export const importApi = (): ImportApi => {
	if (!shared) {
		const lazy = async () => (await import('@/plugins/axios')).default;
		shared = createImportApi({
			get: async (url) => (await lazy()).get(url),
			post: async (url, body) => (await lazy()).post(url, body),
			delete: async (url) => (await lazy()).delete(url),
		});
	}
	return shared;
};
