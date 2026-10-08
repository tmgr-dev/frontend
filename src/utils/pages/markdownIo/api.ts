import { uploadPageFile } from '@/actions/tmgr/files';
import {
	getPage,
	getPagesTree,
	invalidatePages,
	PageConflictError,
	type CreatePagePayload,
	type Page,
	type UpdatePagePayload,
} from '@/actions/tmgr/pages';
import $axios from '@/plugins/axios';
import store from '@/store';
import {
	buildExportBundle,
	buildExportTree,
	exportSinglePage,
	type ExportProvider,
} from './exportBundle';
import { buildImportPlan } from './importPlan';
import { runImportWith, type ImportApi } from './importRun';
import { sanitizeFileName } from './paths';
import type {
	ExportResult,
	ImportOptions,
	ImportPlan,
	ImportResult,
	ReadFilesResult,
	VirtualFile,
} from './types';
import { readVirtualFiles } from './zip';

const currentWorkspaceCode = (): string =>
	(store.getters.currentWorkspace?.code as string | undefined) ?? '';

const unwrap = async <T>(request: Promise<any>): Promise<T> => {
	const {
		data: { data },
	} = await request;
	return data;
};

const rethrowConflict = (error: any): never => {
	const response = error?.response;
	if (response?.status === 409 && response.data?.error === 'page_conflict') {
		throw new PageConflictError(response.data.data);
	}
	throw error;
};

const provider: ExportProvider = {
	getPage: (id) => getPage(id, false),
	fetchFile: async (id) => {
		const { data } = await $axios.get(`/files/${id}/content`, {
			responseType: 'blob',
		});
		return new Uint8Array(await (data as Blob).arrayBuffer());
	},
};

const zipResult = (bundle: {
	fileName: string;
	bytes: Uint8Array;
	warnings: ExportResult['warnings'];
}): ExportResult => ({
	fileName: bundle.fileName,
	blob: new Blob([bundle.bytes as BlobPart], { type: 'application/zip' }),
	warnings: bundle.warnings,
});

export const exportPage = async (pageId: number): Promise<ExportResult> => {
	const page = await getPage(pageId, false);
	const { fileName, text } = exportSinglePage(page, currentWorkspaceCode());
	return {
		fileName,
		blob: new Blob([text], { type: 'text/markdown;charset=utf-8' }),
		warnings: [],
	};
};

export const exportSubtree = async (rootId: number): Promise<ExportResult> => {
	const roots = buildExportTree(await getPagesTree(false), rootId);
	if (!roots.length) throw new Error('Page not found');
	return zipResult(
		await buildExportBundle({
			roots,
			workspaceCode: currentWorkspaceCode(),
			zipName: `${sanitizeFileName(roots[0].summary.slug)}.zip`,
			provider,
		}),
	);
};

export const exportWorkspace = async (
	workspaceCode: string,
): Promise<ExportResult> =>
	zipResult(
		await buildExportBundle({
			roots: buildExportTree(await getPagesTree(false), null),
			workspaceCode,
			zipName: `${sanitizeFileName(workspaceCode)}-pages.zip`,
			provider,
		}),
	);

export const readImportFiles = async (
	files: File[],
): Promise<ReadFilesResult> =>
	readVirtualFiles(
		await Promise.all(
			files.map(async (file) => ({
				name: file.webkitRelativePath || file.name,
				bytes: new Uint8Array(await file.arrayBuffer()),
			})),
		),
	);

const childTitles = async (parentId: number | null): Promise<string[]> =>
	(await getPagesTree(false))
		.filter((page) => !page.deleted_at && page.parent_id === parentId)
		.map((page) => page.title);

export const planImport = async (
	files: VirtualFile[],
	target: { parentId: number | null; workspaceCode: string },
): Promise<ImportPlan> =>
	buildImportPlan(files, {
		existingTitles: await childTitles(target.parentId),
	});

const importApi: ImportApi = {
	createPage: (payload: CreatePagePayload) =>
		unwrap<Page>($axios.post('pages', payload)),
	getPage: (id) => getPage(id, false),
	updatePage: async (id: number, payload: UpdatePagePayload) => {
		try {
			return await unwrap<Page>($axios.patch(`pages/${id}`, payload));
		} catch (error) {
			return rethrowConflict(error);
		}
	},
	uploadFile: (pageId, { name, mime, bytes }) =>
		uploadPageFile(pageId, new File([bytes as BlobPart], name, { type: mime })),
	listChildTitles: childTitles,
	finish: () => {
		invalidatePages();
		store.commit('pagesEvent', { type: 'page.local', page: null });
	},
};

export const runImport = async (
	plan: ImportPlan,
	options: ImportOptions,
): Promise<ImportResult> => runImportWith(importApi, plan, options);

export const downloadExport = (result: ExportResult): void => {
	const url = URL.createObjectURL(result.blob);
	const link = document.createElement('a');
	link.href = url;
	link.download = result.fileName;
	document.body.appendChild(link);
	link.click();
	link.remove();
	setTimeout(() => URL.revokeObjectURL(url), 1000);
};
