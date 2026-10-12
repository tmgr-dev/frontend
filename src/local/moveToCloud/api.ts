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
	status: ImportStatusName;
	upload_url?: string;
	upload_headers?: Record<string, string>;
}

export interface FilePresignRequest {
	sha256: string;
	size: number;
	mime: string;
}

export interface PresignedUpload {
	sha256: string;
	status: 'pending' | 'uploaded';
	upload_url?: string;
	upload_headers?: Record<string, string>;
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
	errors: ImportError[];
	report: ImportReport | null;
	workspace_id?: number | null;
	verified?: boolean;
	source_revision?: string;
}

export interface ImportError {
	phase?: string;
	code?: string;
	entity?: string;
	expected?: number;
	actual?: number;
	imported?: number;
	skipped?: number;
	count?: number;
	message?: string;
}

export interface ImportReport {
	counts: Record<
		string,
		{ expected: number; imported: number; skipped: number }
	>;
	skipped_files: { local_id: string; name: string; reason: string }[];
	unmapped_refs: number;
}

export interface Http {
	get<T = any>(url: string): Promise<{ data: T }>;
	post<T = any>(url: string, body?: unknown): Promise<{ data: T }>;
	delete<T = any>(url: string): Promise<{ data: T }>;
}

export const UNSUPPORTED_SCHEMA_MESSAGE = 'Update TMGR to move this workspace.';
export const QUOTA_MESSAGE =
	'You can start up to 3 moves a day. Try again tomorrow.';
export const RUNNING_MESSAGE = 'Another move is still running.';
export const TOO_LARGE_MESSAGE =
	'This workspace is too large to move in one go.';

export interface ImportApi {
	capabilities(): Promise<ImportCapabilities>;
	createJob(request: CreateJobRequest): Promise<CreatedJob>;
	bundleUploaded(jobId: string): Promise<ImportStatus>;
	presignFile(
		jobId: string,
		file: FilePresignRequest,
	): Promise<PresignedUpload>;
	confirmFile(jobId: string, sha256: string): Promise<void>;
	commit(jobId: string): Promise<ImportStatus>;
	status(jobId: string): Promise<ImportStatus>;
	retry(jobId: string): Promise<ImportStatus>;
	publish(jobId: string, acceptDifferences?: boolean): Promise<ImportStatus>;
	discard(jobId: string): Promise<void>;
}

const BASE = '/imports';

const unwrap = <T>(response: { data: any }): T => response.data?.data as T;

export const createImportApi = (http: Http): ImportApi => {
	const job = (id: string) => `${BASE}/${encodeURIComponent(id)}`;
	return {
		capabilities: async () =>
			unwrap(await http.get(`${BASE}/local-workspace/capabilities`)),
		createJob: async (request) =>
			unwrap(await http.post(`${BASE}/local-workspace`, request)),
		bundleUploaded: async (id) =>
			unwrap(await http.post(`${job(id)}/bundle/uploaded`)),
		presignFile: async (id, file) =>
			unwrap(await http.post(`${job(id)}/files/presign`, file)),
		confirmFile: async (id, sha256) => {
			await http.post(`${job(id)}/files/confirm`, { sha256 });
		},
		commit: async (id) => unwrap(await http.post(`${job(id)}/commit`)),
		status: async (id) => unwrap(await http.get(job(id))),
		retry: async (id) => unwrap(await http.post(`${job(id)}/retry`)),
		publish: async (id, acceptDifferences = false) =>
			unwrap(
				await http.post(
					`${job(id)}/publish`,
					acceptDifferences ? { accept_differences: true } : {},
				),
			),
		discard: async (id) => {
			await http.delete(job(id));
		},
	};
};

export const errorCode = (error: unknown): string | undefined =>
	(error as any)?.response?.data?.error;

export const isUnsupportedSchema = (error: unknown): boolean => {
	const response = (error as any)?.response;
	return (
		response?.status === 422 &&
		response?.data?.error === 'unsupported_bundle_schema'
	);
};

export const isFileNotAllowed = (error: unknown): boolean =>
	(error as any)?.response?.status === 422 &&
	errorCode(error) === 'file_not_allowed';

export const apiErrorMessage = (error: unknown): string => {
	if (isUnsupportedSchema(error)) return UNSUPPORTED_SCHEMA_MESSAGE;
	const code = errorCode(error);
	if (code === 'import_quota') return QUOTA_MESSAGE;
	if (code === 'import_running') return RUNNING_MESSAGE;
	if ((error as any)?.response?.status === 413) return TOO_LARGE_MESSAGE;
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
