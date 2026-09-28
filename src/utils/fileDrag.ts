export const isFileDrag = (event: Pick<DragEvent, 'dataTransfer'>): boolean =>
	Array.from(event.dataTransfer?.types ?? []).includes('Files');

export const createFileDragDepth = () => {
	let depth = 0;

	return {
		enter(): boolean {
			depth += 1;
			return true;
		},
		leave(): boolean {
			depth = Math.max(0, depth - 1);
			return depth > 0;
		},
		reset(): void {
			depth = 0;
		},
	};
};

// With the native Tauri drop handler off, a file dropped outside any drop zone makes the
// webview navigate to it and the app is gone.
export const installFileDropGuard = (
	target: Pick<Window, 'addEventListener'>,
): void => {
	const guard = (event: DragEvent) => {
		if (isFileDrag(event)) {
			event.preventDefault();
		}
	};
	target.addEventListener('dragover', guard);
	target.addEventListener('drop', guard);
};
