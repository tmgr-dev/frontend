import {
	attachFileToPage,
	presignUpload,
	putToStorage,
} from '@/actions/tmgr/files';
import {
	invalidatePages,
	PageConflictError,
	type CreatePagePayload,
	type Page,
	type PageSummary,
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
import {
	WorkspaceChangedError,
	type ExportResult,
	type ImportOptions,
	type ImportPlan,
	type ImportResult,
	type ReadFilesResult,
	type VirtualFile,
} from './types';
import { readRawFiles } from './zip';

const currentWorkspaceCode = (): string =>
	(store.getters.currentWorkspace?.code as string | undefined) ?? '';

const currentWorkspaceId = (): number | null => {
	const id = store.getters.currentWorkspaceId;
	return id == null || id === '' ? null : Number(id);
};

interface Pin {
	id: number | null;
	assert(): void;
	config(extra?: Record<string, any>): Record<string, any>;
}

const pinWorkspace = (
	operation: 'import' | 'export',
	pinned?: number | null,
): Pin => {
	const id = pinned === undefined ? currentWorkspaceId() : pinned;
	return {
		id,
		assert: () => {
			if (currentWorkspaceId() !== id)
				throw new WorkspaceChangedError(operation);
		},
		config: (extra = {}) =>
			id === null
				? extra
				: {
						...extra,
						params: { workspace_id: id },
						...(id > 0 ? { headers: { 'X-Workspace-Id': String(id) } } : {}),
				  },
	};
};

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

const readPage = (id: number | string, pin: Pin): Promise<Page> => {
	pin.assert();
	return unwrap<Page>(
		$axios.get(`pages/${encodeURIComponent(id)}`, pin.config()),
	);
};

const readTree = (pin: Pin): Promise<PageSummary[]> => {
	pin.assert();
	return unwrap<PageSummary[]>($axios.get('pages/tree', pin.config()));
};

const exportProvider = (pin: Pin): ExportProvider => ({
	getPage: (id) => readPage(id, pin),
	fetchFile: async (id) => {
		pin.assert();
		const { data } = await $axios.get(
			`/files/${id}/content`,
			pin.config({ responseType: 'blob' }),
		);
		return new Uint8Array(await (data as Blob).arrayBuffer());
	},
});

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
	const pin = pinWorkspace('export');
	const page = await readPage(pageId, pin);
	pin.assert();
	const { fileName, text } = exportSinglePage(page, currentWorkspaceCode());
	return {
		fileName,
		blob: new Blob([text], { type: 'text/markdown;charset=utf-8' }),
		warnings: [],
	};
};

export const exportSubtree = async (rootId: number): Promise<ExportResult> => {
	const pin = pinWorkspace('export');
	const roots = buildExportTree(await readTree(pin), rootId);
	if (!roots.length) throw new Error('Page not found');
	const bundle = await buildExportBundle({
		roots,
		workspaceCode: currentWorkspaceCode(),
		zipName: `${sanitizeFileName(roots[0].summary.slug)}.zip`,
		provider: exportProvider(pin),
	});
	pin.assert();
	return zipResult(bundle);
};

export const exportWorkspace = async (
	workspaceCode: string,
): Promise<ExportResult> => {
	const pin = pinWorkspace('export');
	const bundle = await buildExportBundle({
		roots: buildExportTree(await readTree(pin), null),
		workspaceCode,
		zipName: `${sanitizeFileName(workspaceCode)}-pages.zip`,
		provider: exportProvider(pin),
	});
	pin.assert();
	return zipResult(bundle);
};

export const readImportFiles = (files: File[]): Promise<ReadFilesResult> =>
	readRawFiles(files);

const childTitles = async (
	parentId: number | null,
	pin: Pin = pinWorkspace('import'),
): Promise<string[]> =>
	(await readTree(pin))
		.filter((page) => !page.deleted_at && page.parent_id === parentId)
		.map((page) => page.title);

export const planImport = async (
	files: VirtualFile[],
	target: { parentId: number | null; workspaceCode: string },
): Promise<ImportPlan> => {
	const pin = pinWorkspace('import');
	const plan = buildImportPlan(files, {
		existingTitles: await childTitles(target.parentId, pin),
	});
	return { ...plan, workspaceId: pin.id };
};

const importApi = (pin: Pin): ImportApi => ({
	createPage: async (payload: CreatePagePayload) => {
		pin.assert();
		return unwrap<Page>($axios.post('pages', payload, pin.config()));
	},
	getPage: (id) => readPage(id, pin),
	updatePage: async (id: number, payload: UpdatePagePayload) => {
		pin.assert();
		try {
			return await unwrap<Page>(
				$axios.patch(`pages/${id}`, payload, pin.config()),
			);
		} catch (error) {
			return rethrowConflict(error);
		}
	},
	uploadFile: async (pageId, { name, mime, bytes }) => {
		const file = new File([bytes as BlobPart], name, { type: mime });
		const workspaceId = pin.id ?? undefined;
		pin.assert();
		const target = await presignUpload(file, workspaceId);
		pin.assert();
		await putToStorage(target, file);
		pin.assert();
		return attachFileToPage(pageId, file, target, workspaceId);
	},
	listChildTitles: (parentId) => childTitles(parentId, pin),
	finish: () => {
		invalidatePages();
		store.commit('pagesEvent', { type: 'page.local', page: null });
	},
});

export const runImport = async (
	plan: ImportPlan,
	options: ImportOptions,
): Promise<ImportResult> =>
	runImportWith(
		importApi(pinWorkspace('import', plan.workspaceId)),
		plan,
		options,
	);

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
