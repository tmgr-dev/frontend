import type { PageType } from '@/actions/tmgr/pages';

export const IMPORT_LIMITS = {
	maxEntries: 500,
	maxInputBytes: 50 * 1024 * 1024,
	maxTotalBytes: 200 * 1024 * 1024,
	maxEntryBytes: 20 * 1024 * 1024,
	maxBodyBytes: 1_048_576,
};

export type ImportLimits = typeof IMPORT_LIMITS;

export interface VirtualFile {
	path: string;
	bytes: Uint8Array;
}

export interface IoWarning {
	path: string | null;
	message: string;
}

export interface ReadFilesResult {
	files: VirtualFile[];
	warnings: IoWarning[];
}

export type ConflictPolicy = 'skip' | 'rename' | 'import';

export interface PlannedPage {
	key: string;
	path: string | null;
	parentKey: string | null;
	title: string;
	type: PageType;
	properties: Record<string, any> | null;
	body: string;
	sourceId: number | null;
	sourceWorkspace: string | null;
	conflict: boolean;
}

export interface ImportPlan {
	pages: PlannedPage[];
	files: Record<string, VirtualFile>;
	warnings: IoWarning[];
}

export interface ImportOptions {
	parentId: number | null;
	policy: ConflictPolicy;
	workspaceCode: string;
	onProgress?: (progress: ImportProgress) => void;
}

export interface ImportProgress {
	done: number;
	total: number;
	current: string;
}

export interface CreatedPage {
	key: string;
	id: number;
	slug: string;
	title: string;
}

export interface ImportResult {
	created: CreatedPage[];
	skipped: string[];
	incomplete: string[];
	warnings: IoWarning[];
	error: string | null;
}

export interface ExportResult {
	fileName: string;
	blob: Blob;
	warnings: IoWarning[];
}

export class WorkspaceChangedError extends Error {
	constructor(operation: 'import' | 'export') {
		super(`The workspace changed during ${operation}`);
		this.name = 'WorkspaceChangedError';
	}
}
