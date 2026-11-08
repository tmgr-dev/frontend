import { t } from '@/i18n';

export interface CategoryTransferStatusMapping {
	from_id: number;
	from_name: string;
	to_id: number | null;
	to_name: string | null;
	tasks: number;
}

export interface CategoryTransferResult {
	category: { id: number; title: string; workspace_id: number };
	tasks_moved: number;
	assignees_removed: number;
	status_mappings: CategoryTransferStatusMapping[];
}

/** One-line, human-readable summary of what a category transfer did, for the toast. */
export const describeCategoryTransfer = (
	result: CategoryTransferResult,
): string => {
	const parts = [
		t('board.helpers.categoryTransfer.tasksMoved', result.tasks_moved),
	];

	if (result.status_mappings.length > 0) {
		const mappings = result.status_mappings
			.map((m) =>
				t('board.helpers.categoryTransfer.mapping', {
					from: m.from_name,
					to: m.to_name ?? t('board.helpers.categoryTransfer.noStatus'),
					tasks: m.tasks,
				}),
			)
			.join(', ');
		parts.push(t('board.helpers.categoryTransfer.statuses', { mappings }));
	}

	if (result.assignees_removed > 0) {
		parts.push(
			t(
				'board.helpers.categoryTransfer.assigneesRemoved',
				result.assignees_removed,
			),
		);
	}

	return parts.join(' ');
};
