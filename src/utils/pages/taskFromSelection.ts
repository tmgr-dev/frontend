import { t } from '@/i18n';
import { titleKey } from '@/utils/taskKeys';

export const LAST_CATEGORY_KEY = 'tmgr.pages.taskFromSelection.category';

export const taskKeyLabel = (task: {
	id: number;
	key?: string | null;
	title?: string | null;
}): string => task.key || titleKey(task.title) || `#${task.id}`;

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
		return t('pages.helpers.taskFromSelection.selectionNotFound');
	}
	if (error?.name === 'PageConflictError') {
		return t('pages.helpers.taskFromSelection.pageChanged');
	}
	if (error?.response?.status === 403)
		return t('pages.helpers.taskFromSelection.noAccess');
	return t('pages.helpers.taskFromSelection.failed');
};
