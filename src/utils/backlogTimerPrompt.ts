/**
 * Starting a timer on a task that still sits in a backlog ("default" type) status is
 * almost always a mistake, so every start entry point (task form, list, board card)
 * offers to move it to the first active status. This is the shared decision + copy.
 */
export interface PromptStatus {
	id: number;
	name: string;
	type: string;
}

export interface BacklogTimerPrompt<S extends PromptStatus = PromptStatus> {
	targetStatus: S;
	title: string;
	message: string;
}

export function backlogTimerPrompt<S extends PromptStatus>(
	task: { title?: string | null; status_id?: number | null },
	statuses: S[] | null | undefined,
): BacklogTimerPrompt<S> | null {
	if (!statuses || task.status_id == null) return null;
	const current = statuses.find((s) => s.id === task.status_id);
	if (!current || current.type !== 'default') return null;
	const targetStatus = statuses.find((s) => s.type === 'active');
	if (!targetStatus) return null;
	return {
		targetStatus,
		title: 'Task in Backlog',
		message: `Task "${task.title ?? ''}" is in backlog. Switch to "${targetStatus.name}" status?`,
	};
}
