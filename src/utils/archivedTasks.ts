export interface ArchivedStatusSets {
	ids: Set<number>;
	names: Set<string>;
}

interface ArchivableStatus {
	id?: number | string | null;
	name?: string | null;
	type?: string | null;
}

interface ArchivableTask {
	status_id?: number | string | null;
	status?: string | null;
}

export function buildArchivedStatusSets(
	statuses: ArchivableStatus[] | null | undefined,
): ArchivedStatusSets {
	const ids = new Set<number>();
	const names = new Set<string>();

	if (!statuses) {
		return { ids, names };
	}

	for (const status of statuses) {
		if (status?.type !== 'archived') {
			continue;
		}
		if (status.id != null) {
			const id = Number(status.id);
			if (!Number.isNaN(id)) {
				ids.add(id);
			}
		}
		if (status.name) {
			names.add(status.name);
		}
	}

	return { ids, names };
}

export function isArchivedTask(
	task: ArchivableTask | null | undefined,
	sets: ArchivedStatusSets,
): boolean {
	if (!task) {
		return false;
	}

	if (task.status_id != null) {
		const statusId = Number(task.status_id);
		if (!Number.isNaN(statusId) && sets.ids.has(statusId)) {
			return true;
		}
	}

	return task.status != null && sets.names.has(task.status);
}
