export interface StatusLike {
	id: number | string;
	type?: string | null;
}

export interface TaskLike {
	status_id?: number | string | null;
}

const CLOSED_TYPES = ['completed', 'archived', 'hidden'];

export const isDone = (status?: Pick<StatusLike, 'type'> | null): boolean =>
	status?.type === 'completed';

export const isArchived = (status?: Pick<StatusLike, 'type'> | null): boolean =>
	status?.type === 'archived';

export const stateLabel = (
	status?: Pick<StatusLike, 'type'> | null,
): 'Done' | 'Archived' | 'Open' =>
	isDone(status) ? 'Done' : isArchived(status) ? 'Archived' : 'Open';

const firstId = (
	statuses: StatusLike[] | null | undefined,
	match: (status: StatusLike) => boolean,
): number | null => {
	const found = (statuses || []).find(match);
	return found ? Number(found.id) : null;
};

export const openTarget = (
	statuses: StatusLike[] | null | undefined,
): number | null =>
	firstId(statuses, (s) => s.type === 'default') ??
	firstId(statuses, (s) => !CLOSED_TYPES.includes(String(s.type)));

export const doneTarget = (
	statuses: StatusLike[] | null | undefined,
): number | null => firstId(statuses, isDone);

export const archiveTarget = (
	statuses: StatusLike[] | null | undefined,
): number | null => firstId(statuses, isArchived);

const statusOfTask = (
	task: TaskLike | null | undefined,
	statuses: StatusLike[] | null | undefined,
): StatusLike | undefined => {
	if (task?.status_id == null) return undefined;
	return (statuses || []).find((s) => Number(s.id) === Number(task.status_id));
};

export const isTaskDone = (
	task: TaskLike | null | undefined,
	statuses: StatusLike[] | null | undefined,
): boolean => isDone(statusOfTask(task, statuses));

export const isTaskArchived = (
	task: TaskLike | null | undefined,
	statuses: StatusLike[] | null | undefined,
): boolean => isArchived(statusOfTask(task, statuses));

export const toggleDoneTarget = (
	task: TaskLike | null | undefined,
	statuses: StatusLike[] | null | undefined,
): number | null =>
	isTaskDone(task, statuses) ? openTarget(statuses) : doneTarget(statuses);

export const DONE_DISABLED_TOOLTIP = 'This workspace has no Done status';

type IsOn = (key: string) => boolean;

export const isSimpleMode = (isOn: IsOn): boolean => !isOn('workflow');

export const isBoardVisible = (isOn: IsOn): boolean =>
	isOn('board') && isOn('workflow');
