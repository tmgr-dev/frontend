import {
	closePagesImport,
	isFileDrag,
	openPagesImport,
	pagesImportRequest,
	trackInternalDrags,
	useFileDrop,
} from '../usePagesMarkdownIo';

jest.mock('@/components/ui/toast', () => ({
	useToast: () => ({ toast: jest.fn() }),
}));

const dragEvent = (types: string[], files: File[] = []) => ({
	dataTransfer: { types, files, dropEffect: 'none' },
	preventDefault: jest.fn(),
	stopPropagation: jest.fn(),
});

describe('drags that start inside the document', () => {
	it('are not treated as file drags until they end', () => {
		jest.useFakeTimers();
		const target = new EventTarget();
		trackInternalDrags(target);
		const files = dragEvent(['Files']) as any;
		expect(isFileDrag(files)).toBe(true);
		target.dispatchEvent(new Event('dragstart'));
		expect(isFileDrag(files)).toBe(false);
		target.dispatchEvent(new Event('dragend'));
		expect(isFileDrag(files)).toBe(true);
		target.dispatchEvent(new Event('dragstart'));
		target.dispatchEvent(new Event('drop'));
		expect(isFileDrag(files)).toBe(false);
		jest.runAllTimers();
		expect(isFileDrag(files)).toBe(true);
		jest.useRealTimers();
	});
});

describe('file drop handling', () => {
	it('detects only OS file drags', () => {
		expect(isFileDrag(dragEvent(['Files']) as any)).toBe(true);
		expect(isFileDrag(dragEvent(['text/plain']) as any)).toBe(false);
		expect(isFileDrag({ dataTransfer: null })).toBe(false);
	});

	it('ignores non-file drags without touching the event', () => {
		const onFiles = jest.fn();
		const { handlers, dragging } = useFileDrop(onFiles);
		const event = dragEvent(['text/plain']);
		for (const handler of Object.values(handlers)) handler(event as any);
		expect(event.preventDefault).not.toHaveBeenCalled();
		expect(event.stopPropagation).not.toHaveBeenCalled();
		expect(dragging.value).toBe(false);
		expect(onFiles).not.toHaveBeenCalled();
	});

	it('highlights during a file drag and hands dropped files over', () => {
		const onFiles = jest.fn();
		const { handlers, dragging } = useFileDrop(onFiles);
		const file = new File(['x'], 'a.md');
		handlers.dragenter(dragEvent(['Files']) as any);
		expect(dragging.value).toBe(true);
		const drop = dragEvent(['Files'], [file]);
		handlers.drop(drop as any);
		expect(drop.preventDefault).toHaveBeenCalled();
		expect(dragging.value).toBe(false);
		expect(onFiles).toHaveBeenCalledWith([file]);
	});

	it('keeps the highlight until the last nested dragleave', () => {
		const { handlers, dragging } = useFileDrop(jest.fn());
		const event = dragEvent(['Files']) as any;
		handlers.dragenter(event);
		handlers.dragenter(event);
		handlers.dragleave(event);
		expect(dragging.value).toBe(true);
		handlers.dragleave(event);
		expect(dragging.value).toBe(false);
	});
});

describe('import request state', () => {
	it('opens with files and parent, then closes', () => {
		const file = new File(['x'], 'a.md');
		openPagesImport({
			workspaceCode: 'ws',
			parent: { id: 3, title: 'Parent' },
			files: [file],
		});
		expect(pagesImportRequest).toMatchObject({
			open: true,
			parentId: 3,
			parentTitle: 'Parent',
			workspaceCode: 'ws',
		});
		expect(pagesImportRequest.files).toEqual([file]);
		closePagesImport();
		expect(pagesImportRequest.open).toBe(false);
	});
});
