import { createDomainEvents } from '@/utils/domainEvents';
import type { EventHandlers } from '@/types/dashboard';
import { installLocalLiveUpdates, reportAgentWorkRun, type LiveUpdateDeps } from '../liveUpdates';

const flush = () => new Promise((resolve) => setTimeout(resolve, 0));

describe('installLocalLiveUpdates', () => {
	let bus: ReturnType<typeof createDomainEvents>;
	let handlers: Required<Pick<EventHandlers, 'onTaskUpdated' | 'onCommentAdded' | 'onCommentUpdated' | 'onCommentDeleted' | 'onCommentReactionsUpdated' | 'onAgentWorkChanged'>>;
	let deps: LiveUpdateDeps & { deliver: jest.Mock; fetchTask: jest.Mock; invalidate: jest.Mock };
	let uninstall: () => void;

	beforeEach(() => {
		bus = createDomainEvents();
		handlers = {
			onTaskUpdated: jest.fn(),
			onCommentAdded: jest.fn(),
			onCommentUpdated: jest.fn(),
			onCommentDeleted: jest.fn(),
			onCommentReactionsUpdated: jest.fn(),
			onAgentWorkChanged: jest.fn(),
		};
		deps = {
			deliver: jest.fn((workspaceId: number, call: (h: EventHandlers) => void) => {
				if (workspaceId === -3) call(handlers);
			}),
			fetchTask: jest.fn(async (_workspaceId: number, id: number) => ({ id, title: `fresh ${id}` })),
			invalidate: jest.fn(),
		};
		uninstall = installLocalLiveUpdates(deps, bus);
	});

	afterEach(() => uninstall());

	it('hands a persona task update to the workspace onTaskUpdated handlers and drops the cached task', () => {
		bus.emit({ type: 'task.updated', workspaceId: -3, taskId: 5, task: { id: 5, title: 'x' }, actor: 'persona:p-1' });
		expect(handlers.onTaskUpdated).toHaveBeenCalledWith({ id: 5, title: 'x' }, 'updated');
		expect(deps.invalidate).toHaveBeenCalledWith('task-5');
		expect(deps.invalidate).toHaveBeenCalledWith(/^tasks-status-/);
	});

	it('maps task.created and task.deleted to their actions', () => {
		bus.emit({ type: 'task.created', workspaceId: -3, taskId: 6, task: { id: 6 }, actor: 'persona:p-1' });
		bus.emit({ type: 'task.deleted', workspaceId: -3, taskId: 6, actor: 'persona:p-1' });
		expect(handlers.onTaskUpdated).toHaveBeenNthCalledWith(1, { id: 6 }, 'created');
		expect(handlers.onTaskUpdated).toHaveBeenNthCalledWith(2, { id: 6 }, 'deleted');
	});

	it('delivers a status change that came with task.updated only once', () => {
		const task = { id: 5, status_id: 2 };
		bus.emit({ type: 'task.updated', workspaceId: -3, taskId: 5, task, changed: ['status_id'], actor: 'persona:p-1' });
		bus.emit({ type: 'task.statusChanged', workspaceId: -3, taskId: 5, statusId: 2, task, actor: 'persona:p-1' });
		expect(handlers.onTaskUpdated).toHaveBeenCalledTimes(1);
	});

	it('refetches a task whose status changed without a payload', async () => {
		bus.emit({ type: 'task.statusChanged', workspaceId: -3, taskId: 8, statusId: 2, actor: 'plugin:x' });
		await flush();
		expect(deps.fetchTask).toHaveBeenCalledWith(-3, 8);
		expect(handlers.onTaskUpdated).toHaveBeenCalledWith({ id: 8, title: 'fresh 8' }, 'updated');
	});

	it('hands comments to the comment handlers and drops the cached comment list', () => {
		const comment = { id: 11, task_id: 5, message: 'hi' };
		bus.emit({ type: 'comment.created', workspaceId: -3, taskId: 5, comment, actor: 'persona:p-1' });
		bus.emit({ type: 'comment.updated', workspaceId: -3, taskId: 5, comment, actor: 'persona:p-1' });
		bus.emit({ type: 'comment.deleted', workspaceId: -3, commentId: 11, taskId: 5, actor: 'persona:p-1' });
		expect(handlers.onCommentAdded).toHaveBeenCalledWith(comment);
		expect(handlers.onCommentUpdated).toHaveBeenCalledWith(comment);
		expect(handlers.onCommentDeleted).toHaveBeenCalledWith({ id: 11, task_id: 5 });
		expect(deps.invalidate).toHaveBeenCalledWith('comments-task-5');
	});

	it('hands reactions over in the realtime event shape', () => {
		const reactions = [{ emoji: '👍', count: 1, reacted: true, users: [] }];
		bus.emit({ type: 'comment.reactionChanged', workspaceId: -3, commentId: 11, taskId: 5, reactions, actor: 'persona:p-1' });
		expect(handlers.onCommentReactionsUpdated).toHaveBeenCalledWith({ comment_id: 11, task_id: 5, reactions });
	});

	it('refetches both tasks of a changed relation', async () => {
		bus.emit({
			type: 'task.relationChanged',
			workspaceId: -3,
			taskId: 5,
			otherTaskId: 9,
			relationType: 'blocks',
			change: 'added',
			actor: 'persona:p-1',
		});
		await flush();
		expect(deps.invalidate).toHaveBeenCalledWith('task-5');
		expect(deps.invalidate).toHaveBeenCalledWith('task-9');
		expect(handlers.onTaskUpdated).toHaveBeenCalledWith({ id: 5, title: 'fresh 5' }, 'updated');
		expect(handlers.onTaskUpdated).toHaveBeenCalledWith({ id: 9, title: 'fresh 9' }, 'updated');
	});

	it('delivers nothing when the refetch is refused for another workspace', async () => {
		deps.fetchTask.mockResolvedValue(null);
		bus.emit({ type: 'task.statusChanged', workspaceId: -3, taskId: 8, statusId: 2, actor: 'plugin:x' });
		await flush();
		expect(deps.deliver).not.toHaveBeenCalled();
	});

	it('leaves writes made by the UI itself alone', () => {
		bus.emit({ type: 'task.updated', workspaceId: -3, taskId: 5, task: { id: 5 } });
		bus.emit({ type: 'comment.created', workspaceId: -3, taskId: 5, comment: { id: 1, task_id: 5 } });
		bus.emit({ type: 'comment.deleted', workspaceId: -3, commentId: 11, taskId: 5 });
		expect(deps.deliver).not.toHaveBeenCalled();
	});

	it('still delivers a comment.deleted event with no task_id (backward compatible with older emitters)', () => {
		bus.emit({ type: 'comment.deleted', workspaceId: -3, commentId: 11, actor: 'persona:p-1' });
		expect(handlers.onCommentDeleted).toHaveBeenCalledWith({ id: 11 });
	});

	it('leaves cloud workspaces to the realtime socket', () => {
		bus.emit({ type: 'task.updated', workspaceId: 56, taskId: 5, task: { id: 5 }, actor: 'plugin:x' });
		expect(deps.deliver).not.toHaveBeenCalled();
	});

	it('never delivers timer events', () => {
		bus.emit({ type: 'timer.started', workspaceId: -3, taskId: 5, task: { id: 5 }, actor: 'plugin:x' });
		expect(deps.deliver).not.toHaveBeenCalled();
	});

	it('hands a reported agent work run to onAgentWorkChanged until uninstalled', () => {
		const run = { id: 3, task_id: 5 } as any;
		reportAgentWorkRun(-3, run);
		expect(handlers.onAgentWorkChanged).toHaveBeenCalledWith(run);
		uninstall();
		reportAgentWorkRun(-3, run);
		expect(handlers.onAgentWorkChanged).toHaveBeenCalledTimes(1);
	});
});
