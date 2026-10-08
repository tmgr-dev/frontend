import { useToast } from '@/components/ui/toast';
import type { ExportResult } from '@/utils/pages/markdownIo/types';
import { reactive, ref } from 'vue';

export interface PagesImportRequest {
	open: boolean;
	key: number;
	files: File[];
	parentId: number | null;
	parentTitle: string | null;
	workspaceCode: string;
}

export const pagesImportRequest = reactive<PagesImportRequest>({
	open: false,
	key: 0,
	files: [],
	parentId: null,
	parentTitle: null,
	workspaceCode: '',
});

export const openPagesImport = (options: {
	workspaceCode: string;
	parent?: { id: number; title: string } | null;
	files?: File[];
}) => {
	pagesImportRequest.key += 1;
	pagesImportRequest.files = options.files ?? [];
	pagesImportRequest.parentId = options.parent?.id ?? null;
	pagesImportRequest.parentTitle = options.parent?.title ?? null;
	pagesImportRequest.workspaceCode = options.workspaceCode;
	pagesImportRequest.open = true;
};

export const closePagesImport = () => {
	pagesImportRequest.open = false;
};

let internalDrag = false;

export const trackInternalDrags = (target: EventTarget) => {
	const clear = () => {
		internalDrag = false;
	};
	target.addEventListener(
		'dragstart',
		() => {
			internalDrag = true;
		},
		true,
	);
	target.addEventListener('dragend', clear, true);
	target.addEventListener('drop', () => setTimeout(clear, 0), true);
};

if (typeof document !== 'undefined') trackInternalDrags(document);

export const isFileDrag = (event: { dataTransfer?: DataTransfer | null }) =>
	!internalDrag &&
	Array.from(event.dataTransfer?.types ?? []).includes('Files');

export function useFileDrop(onFiles: (files: File[]) => void) {
	const dragging = ref(false);
	let depth = 0;

	const onDragenter = (event: DragEvent) => {
		if (!isFileDrag(event)) return;
		event.preventDefault();
		event.stopPropagation();
		depth += 1;
		dragging.value = true;
	};

	const onDragover = (event: DragEvent) => {
		if (!isFileDrag(event)) return;
		event.preventDefault();
		event.stopPropagation();
		if (event.dataTransfer) event.dataTransfer.dropEffect = 'copy';
	};

	const onDragleave = (event: DragEvent) => {
		if (!isFileDrag(event)) return;
		event.stopPropagation();
		depth = Math.max(0, depth - 1);
		if (depth === 0) dragging.value = false;
	};

	const onDrop = (event: DragEvent) => {
		if (!isFileDrag(event)) return;
		event.preventDefault();
		event.stopPropagation();
		depth = 0;
		dragging.value = false;
		const files = Array.from(event.dataTransfer?.files ?? []);
		if (files.length) onFiles(files);
	};

	return {
		dragging,
		handlers: {
			dragenter: onDragenter,
			dragover: onDragover,
			dragleave: onDragleave,
			drop: onDrop,
		},
	};
}

export function usePagesMarkdownIo(getWorkspaceCode: () => string | undefined) {
	const toaster = useToast();
	const busy = ref(false);

	const runExport = async (
		build: (
			api: typeof import('@/utils/pages/markdownIo/api'),
		) => Promise<ExportResult>,
	) => {
		if (busy.value) return;
		busy.value = true;
		try {
			const api = await import('@/utils/pages/markdownIo/api');
			const result = await build(api);
			api.downloadExport(result);
			if (result.warnings.length) {
				toaster.toast({
					title: `Exported with ${result.warnings.length} warning${
						result.warnings.length === 1 ? '' : 's'
					}`,
					description: result.warnings[0].message,
				});
			}
		} catch (error) {
			toaster.toast({
				title: 'Export failed',
				description: error instanceof Error ? error.message : undefined,
				variant: 'destructive',
			});
		} finally {
			busy.value = false;
		}
	};

	const exportPage = (id: number) => runExport((api) => api.exportPage(id));

	const exportSubtree = (id: number) =>
		runExport((api) => api.exportSubtree(id));

	const exportWorkspace = () => {
		const code = getWorkspaceCode();
		if (code) void runExport((api) => api.exportWorkspace(code));
	};

	const openImport = (
		parent: { id: number; title: string } | null = null,
		files: File[] = [],
	) => {
		const workspaceCode = getWorkspaceCode();
		if (workspaceCode) openPagesImport({ workspaceCode, parent, files });
	};

	return { busy, exportPage, exportSubtree, exportWorkspace, openImport };
}
