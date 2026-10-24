import type { PersonaAssignee } from './personas';

export interface LastTaskSettings {
	status_id: number | null;
	project_category_id: number | null;
	assignees: number[];
	persona_assignees: PersonaAssignee[];
	approximately_time: number;
}

type KeyValueStorage = Pick<Storage, 'getItem' | 'setItem'>;

const storageKey = (workspaceCode: string) =>
	`tmgr:last-task-settings:${workspaceCode}`;

const defaultStorage = (): KeyValueStorage | null =>
	typeof localStorage === 'undefined' ? null : localStorage;

const idOrNull = (value: unknown): number | null =>
	typeof value === 'number' && value > 0 ? value : null;

export const saveLastTaskSettings = (
	workspaceCode: string,
	settings: LastTaskSettings,
	storage: KeyValueStorage | null = defaultStorage(),
): void => {
	if (!workspaceCode || !storage) return;
	try {
		storage.setItem(storageKey(workspaceCode), JSON.stringify(settings));
	} catch {
		// Quota or private mode: remembering settings is a convenience only.
	}
};

export const loadLastTaskSettings = (
	workspaceCode: string,
	storage: KeyValueStorage | null = defaultStorage(),
): LastTaskSettings | null => {
	if (!workspaceCode || !storage) return null;
	let raw: unknown;
	try {
		raw = JSON.parse(storage.getItem(storageKey(workspaceCode)) ?? 'null');
	} catch {
		return null;
	}
	if (!raw || typeof raw !== 'object') return null;
	const value = raw as Record<string, unknown>;
	const time = Number(value.approximately_time);

	return {
		status_id: idOrNull(value.status_id),
		project_category_id: idOrNull(value.project_category_id),
		assignees: Array.isArray(value.assignees)
			? value.assignees.filter((id): id is number => typeof id === 'number')
			: [],
		persona_assignees: Array.isArray(value.persona_assignees)
			? value.persona_assignees.filter(
					(p): p is PersonaAssignee =>
						!!p && typeof p === 'object' && typeof p.id === 'string',
				)
			: [],
		approximately_time: Number.isFinite(time) && time > 0 ? time : 0,
	};
};

/** Drops what no longer exists (a deleted status, category or member) so Apply never sets a dangling id. */
export const applicableTaskSettings = (
	settings: LastTaskSettings,
	known: {
		statusIds: number[];
		categoryIds: number[];
		memberIds: number[];
	},
): LastTaskSettings => ({
	...settings,
	status_id:
		settings.status_id && known.statusIds.includes(settings.status_id)
			? settings.status_id
			: null,
	project_category_id:
		settings.project_category_id &&
		known.categoryIds.includes(settings.project_category_id)
			? settings.project_category_id
			: null,
	assignees: settings.assignees.filter((id) => known.memberIds.includes(id)),
});

export const hasTaskSettings = (settings: LastTaskSettings | null): boolean =>
	!!settings &&
	(!!settings.status_id ||
		!!settings.project_category_id ||
		settings.assignees.length > 0 ||
		settings.persona_assignees.length > 0 ||
		settings.approximately_time > 0);
