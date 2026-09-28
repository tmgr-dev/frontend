import {
	createFileDragDepth,
	installFileDropGuard,
	isFileDrag,
} from '../fileDrag';

const dragOf = (types: string[] | null) =>
	({
		dataTransfer: types ? ({ types } as unknown as DataTransfer) : null,
		preventDefault: jest.fn(),
	} as unknown as DragEvent & { preventDefault: jest.Mock });

describe('isFileDrag', () => {
	it('recognises a drag that carries files', () => {
		expect(isFileDrag(dragOf(['Files']))).toBe(true);
	});

	it('ignores text and in-app drags', () => {
		expect(isFileDrag(dragOf(['text/plain']))).toBe(false);
		expect(isFileDrag(dragOf(null))).toBe(false);
	});
});

describe('createFileDragDepth', () => {
	it('stays active while the pointer moves between nested elements', () => {
		const depth = createFileDragDepth();
		expect(depth.enter()).toBe(true);
		expect(depth.enter()).toBe(true);
		expect(depth.leave()).toBe(true);
		expect(depth.leave()).toBe(false);
	});

	it('never goes below zero on stray leave events', () => {
		const depth = createFileDragDepth();
		expect(depth.leave()).toBe(false);
		expect(depth.enter()).toBe(true);
	});

	it('turns off after reset regardless of depth', () => {
		const depth = createFileDragDepth();
		depth.enter();
		depth.enter();
		depth.reset();
		expect(depth.leave()).toBe(false);
	});
});

describe('installFileDropGuard', () => {
	const install = () => {
		const handlers: Record<string, (event: DragEvent) => void> = {};
		installFileDropGuard({
			addEventListener: (type: string, handler: any) => {
				handlers[type] = handler;
			},
		} as unknown as Window);
		return handlers;
	};

	it('stops the webview from opening a dropped file', () => {
		const handlers = install();
		const over = dragOf(['Files']);
		const drop = dragOf(['Files']);
		handlers.dragover(over);
		handlers.drop(drop);
		expect(over.preventDefault).toHaveBeenCalled();
		expect(drop.preventDefault).toHaveBeenCalled();
	});

	it('leaves in-app drags alone so board drag and drop keeps working', () => {
		const handlers = install();
		const over = dragOf(['text/plain']);
		handlers.dragover(over);
		expect(over.preventDefault).not.toHaveBeenCalled();
	});
});
