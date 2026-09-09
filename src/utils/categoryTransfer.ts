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

const plural = (count: number, noun: string) => `${count} ${noun}${count === 1 ? '' : 's'}`;

/** One-line, human-readable summary of what a category transfer did, for the toast. */
export const describeCategoryTransfer = (result: CategoryTransferResult): string => {
	const parts = [`${plural(result.tasks_moved, 'task')} moved.`];

	if (result.status_mappings.length > 0) {
		const mappings = result.status_mappings
			.map((m) => `${m.from_name} → ${m.to_name ?? 'no status'} (${m.tasks})`)
			.join(', ');
		parts.push(`Statuses: ${mappings}.`);
	}

	if (result.assignees_removed > 0) {
		const who = result.assignees_removed === 1 ? 'not a member' : 'not members';
		parts.push(`${plural(result.assignees_removed, 'assignee')} removed (${who} of the target workspace).`);
	}

	return parts.join(' ');
};
