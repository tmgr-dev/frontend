export const LAST_CATEGORY_KEY = 'tmgr.pages.taskFromSelection.category';

const KEY = /^([A-Z][A-Z0-9]+-\d+)\b/;

export const taskKeyLabel = (task: {
	id: number;
	key?: string | null;
	title?: string | null;
}): string => task.key || KEY.exec(task.title ?? '')?.[1] || `#${task.id}`;

export const pickDefaultCategory = (
	categories: { id: number }[],
	last: number | null,
): number | null => {
	if (last !== null && categories.some((category) => category.id === last)) {
		return last;
	}
	return categories[0]?.id ?? null;
};

export const readLastCategory = (): number | null => {
	try {
		const value = Number(localStorage.getItem(LAST_CATEGORY_KEY));
		return Number.isInteger(value) && value > 0 ? value : null;
	} catch {
		return null;
	}
};

export const rememberCategory = (id: number): void => {
	try {
		localStorage.setItem(LAST_CATEGORY_KEY, String(id));
	} catch {
		return;
	}
};

export const taskFromSelectionError = (error: any): string => {
	const data = error?.response?.data;
	if (
		error?.response?.status === 422 &&
		data?.error === 'selection_not_found'
	) {
		return 'The selected text was not found on the page. Refresh the page and try again.';
	}
	if (error?.name === 'PageConflictError') {
		return 'The page has changed. Refresh it and try again.';
	}
	if (error?.response?.status === 403) return 'No access to the category.';
	return 'Failed to create task.';
};
