import type {
	ExportResult,
	ImportOptions,
	ImportPlan,
	ImportResult,
	ReadFilesResult,
	VirtualFile,
} from './types';

const notImplemented = (): never => {
	throw new Error('not implemented');
};

export const exportPage = async (pageId: number): Promise<ExportResult> =>
	notImplemented();

export const exportSubtree = async (rootId: number): Promise<ExportResult> =>
	notImplemented();

export const exportWorkspace = async (
	workspaceCode: string,
): Promise<ExportResult> => notImplemented();

export const readImportFiles = async (
	files: File[],
): Promise<ReadFilesResult> => notImplemented();

export const planImport = async (
	files: VirtualFile[],
	target: { parentId: number | null; workspaceCode: string },
): Promise<ImportPlan> => notImplemented();

export const runImport = async (
	plan: ImportPlan,
	options: ImportOptions,
): Promise<ImportResult> => notImplemented();

export const downloadExport = (result: ExportResult): void => notImplemented();
