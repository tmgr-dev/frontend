import type { DomainEvent } from '@/utils/domainEvents';

type TaskEvent = Extract<
	DomainEvent,
	{ type: 'task.created' | 'task.updated' | 'task.deleted' }
>;

/** A write made on this instance already produced a local event, so its realtime echo is dropped. */
export const realtimeTaskEvent = (
	workspaceId: number,
	task: Record<string, any> | null | undefined,
	action: 'created' | 'updated' | 'deleted',
	sourceInstanceId: string | undefined,
	ownId: string,
	updatedByPlugin?: string | null,
): TaskEvent | null => {
	if (sourceInstanceId === ownId || sourceInstanceId?.startsWith(`${ownId}.`))
		return null;
	const taskId = task?.id;
	if (!task || taskId == null) return null;
	const actor = updatedByPlugin ? { actor: `plugin:${updatedByPlugin}` } : {};
	if (action === 'deleted')
		return { type: 'task.deleted', workspaceId, taskId, ...actor };
	return {
		type: action === 'created' ? 'task.created' : 'task.updated',
		workspaceId,
		taskId,
		task,
		...actor,
	};
};
