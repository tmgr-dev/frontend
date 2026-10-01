export interface PageWindowTarget {
	pageId: number;
	workspaceCode: string;
	slug: string;
	title: string | null;
}

interface PageLike {
	id?: number | string | null;
	slug?: string | null;
	title?: string | null;
}

export const isPageWindowLabel = (label: string | null): boolean =>
	typeof label === 'string' && label.startsWith('page-');

export const isInPageWindow = (): boolean =>
	isPageWindowLabel(
		(globalThis as any).__TAURI_INTERNALS__?.metadata?.currentWindow?.label ??
			null,
	);

export const pageWindowTarget = (
	page: PageLike | null | undefined,
	workspaceCode: string | null | undefined,
): PageWindowTarget | null => {
	const pageId = Number(page?.id);
	if (!page || !Number.isFinite(pageId) || pageId <= 0) return null;
	if (!workspaceCode || !page.slug) return null;
	return {
		pageId,
		workspaceCode,
		slug: page.slug,
		title: typeof page.title === 'string' && page.title.trim() ? page.title.trim() : null,
	};
};

export const openPageWindow = async (target: PageWindowTarget): Promise<void> => {
	const { invoke } = await import('@tauri-apps/api/core');
	await invoke('open_page_window', { ...target });
};

export const focusPageWindow = async (
	pageId: number,
	workspaceCode: string,
): Promise<boolean> => {
	const { invoke } = await import('@tauri-apps/api/core');
	return invoke<boolean>('focus_page_window', { pageId, workspaceCode });
};

export const setPageWindowTitle = async (title: string): Promise<void> => {
	try {
		const { getCurrentWindow } = await import('@tauri-apps/api/window');
		await getCurrentWindow().setTitle(title.replace(/\s+/g, ' '));
	} catch {
		/* the native title is cosmetic */
	}
};
