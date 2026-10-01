import type { AgentWorkRun } from '@/actions/tmgr/agentWork';
import type { EventHandlers, PageEventPayload } from '@/types/dashboard';
import { domainEvents, eventsForResponse, type DomainEvent } from '@/utils/domainEvents';
import type { LocalRouter } from './router';
import { onTaskAssignment } from './personaAssignees';
import { LocalRaw } from './types';

type AgentWorkListener = (workspaceId: number, run: AgentWorkRun) => void;
const agentWorkListeners = new Set<AgentWorkListener>();

/** Agent work has no domain event (plugins see the whole bus), so its runs travel on their own channel. */
export const reportAgentWorkRun = (workspaceId: number, run: AgentWorkRun) => {
	agentWorkListeners.forEach((listener) => listener(workspaceId, run));
};

const AGENT_WORK_WRITE = /^(tasks\/\d+\/agent-work|agent-work\/\d+(\/finish)?)$/;

interface LiveWriteOptions {
	workspaceId: number;
	personaUuid: string;
	/** The REST path already emits through `installDomainEvents` on its axios client; MCP has no client. */
	emitDomainEvents: boolean;
}

const reportWrite = (
	options: LiveWriteOptions,
	write: { method: string; path: string; body: unknown; data: unknown },
) => {
	if (options.emitDomainEvents) {
		eventsForResponse(
			{ config: { method: write.method, url: write.path, data: write.body }, data: { data: write.data } },
			() => options.workspaceId,
		).forEach((event) => domainEvents.emit({ ...event, actor: `persona:${options.personaUuid}` }));
	}
	const run = write.data as AgentWorkRun | undefined;
	if (AGENT_WORK_WRITE.test(write.path) && run?.task_id != null) {
		reportAgentWorkRun(options.workspaceId, run);
	}
};

/** The same router, but every successful persona write is reported once its handler has committed. */
export const withLiveWrites = (router: LocalRouter, options: LiveWriteOptions): LocalRouter => {
	const wrapped = Object.create(router) as LocalRouter;
	wrapped.match = (method, path) => {
		const route = router.match(method, path);
		if (!route || method.toUpperCase() === 'GET') return route;
		return {
			...route,
			handler: async (req) => {
				const data = await route.handler(req);
				if (!(data instanceof LocalRaw)) {
					try {
						reportWrite(options, { method: req.method, path: req.path, body: req.body, data });
					} catch (error) {
						console.error('[local-access] failed to report a write', error);
					}
				}
				return data;
			},
		};
	};
	return wrapped;
};

export interface LiveUpdateDeps {
	deliver: (workspaceId: number, call: (handlers: EventHandlers) => void) => void;
	/** Resolves null when the workspace is not the one the app shows, so nothing reaches another workspace's API. */
	fetchTask: (workspaceId: number, taskId: number) => Promise<any | null>;
	invalidate: (key: string | RegExp) => void;
}

/**
 * Local workspaces have no realtime socket: writes that did not come from this UI (a persona over
 * the local socket, a plugin) are handed to the same handlers the cloud realtime events call.
 */
export const installLocalLiveUpdates = (deps: LiveUpdateDeps, bus = domainEvents) => {
	const dropTask = (taskId: number) => {
		deps.invalidate(`task-${taskId}`);
		deps.invalidate(/^tasks-status-/);
	};
	const refetchTask = (workspaceId: number, taskId: number) => {
		dropTask(taskId);
		deps
			.fetchTask(workspaceId, taskId)
			.then((task) => {
				if (task) deps.deliver(workspaceId, (h) => h.onTaskUpdated?.(task, 'updated'));
			})
			.catch((error: unknown) => console.error('[local-live] failed to refetch a task', error));
	};
	const apply = (event: DomainEvent, workspaceId: number) => {
		switch (event.type) {
			case 'task.created':
			case 'task.updated':
				dropTask(event.taskId);
				deps.deliver(workspaceId, (h) =>
					h.onTaskUpdated?.(event.task, event.type === 'task.created' ? 'created' : 'updated'),
				);
				return;
			case 'task.deleted':
				dropTask(event.taskId);
				deps.deliver(workspaceId, (h) => h.onTaskUpdated?.({ id: event.taskId }, 'deleted'));
				return;
			case 'task.statusChanged':
				if (!event.task) refetchTask(workspaceId, event.taskId);
				return;
			case 'comment.created':
			case 'comment.updated':
				deps.invalidate(`comments-task-${event.taskId}`);
				deps.deliver(workspaceId, (h) =>
					event.type === 'comment.created' ? h.onCommentAdded?.(event.comment) : h.onCommentUpdated?.(event.comment),
				);
				return;
			case 'comment.deleted':
				deps.deliver(workspaceId, (h) => h.onCommentDeleted?.({ id: event.commentId, task_id: event.taskId }));
				return;
			case 'comment.reactionChanged':
				if (event.taskId == null) return;
				deps.deliver(workspaceId, (h) =>
					h.onCommentReactionsUpdated?.({
						comment_id: event.commentId,
						task_id: event.taskId!,
						reactions: event.reactions,
					}),
				);
				return;
			case 'task.relationChanged':
				refetchTask(workspaceId, event.taskId);
				refetchTask(workspaceId, event.otherTaskId);
				return;
			case 'page.created':
			case 'page.updated':
			case 'page.deleted':
			case 'page.restored':
			case 'page.moved':
				deps.invalidate(/^pages-/);
				deps.deliver(workspaceId, (h) =>
					h.onPageEvent?.(event.type, { page: event.page as PageEventPayload['page'] }),
				);
				return;
		}
	};
	const offBus = bus.on((event) => {
		if (!event.actor) return;
		const workspaceId = event.workspaceId;
		if (workspaceId === null || workspaceId === undefined || workspaceId >= 0) return;
		apply(event, workspaceId);
	});
	const onRun: AgentWorkListener = (workspaceId, run) =>
		deps.deliver(workspaceId, (h) => h.onAgentWorkChanged?.(run));
	agentWorkListeners.add(onRun);
	const offAssignment = onTaskAssignment((report) => {
		if (report.actor === 'user') return;
		refetchTask(report.payload.workspace_id, report.payload.task_id);
	});
	return () => {
		offBus();
		offAssignment();
		agentWorkListeners.delete(onRun);
	};
};
