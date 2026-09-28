import { domainEvents } from '@/utils/domainEvents';
import { installLocalAccessEvents } from '../localAccess';
import { listLocalWorkspaces } from '../runtime';
import type { LocalWorkspace } from '../types';

jest.mock('../runtime', () => ({ listLocalWorkspaces: jest.fn() }));

const workspace: LocalWorkspace = {
	id: -3,
	name: 'Personal',
	code: 'local-personal',
	schema_version: 0,
	created_at: '',
	path: '/tmp/x',
	database: '/tmp/x/workspace.db',
};

const flush = () => new Promise((resolve) => setTimeout(resolve, 0));

describe('installLocalAccessEvents', () => {
	let invoke: jest.Mock;
	let unsubscribe: () => void;

	beforeEach(() => {
		invoke = jest.fn().mockResolvedValue(undefined);
		(listLocalWorkspaces as jest.Mock).mockResolvedValue([workspace]);
		unsubscribe = installLocalAccessEvents(invoke);
	});

	afterEach(() => {
		unsubscribe();
	});

	it('forwards a local workspace event with its permission and the persona actor', async () => {
		domainEvents.emit({ type: 'task.created', workspaceId: -3, taskId: 1, task: { id: 1 }, actor: 'persona:p-1' });
		await flush();
		expect(invoke).toHaveBeenCalledWith('local_access_event', {
			workspaceCode: 'local-personal',
			permission: 'tasks:read',
			event: expect.objectContaining({ type: 'task.created', taskId: 1, actor: 'persona:p-1' }),
		});
	});

	it('defaults actor to "user" when the event has none', async () => {
		domainEvents.emit({ type: 'task.deleted', workspaceId: -3, taskId: 1 });
		await flush();
		expect(invoke).toHaveBeenCalledWith(
			'local_access_event',
			expect.objectContaining({ event: expect.objectContaining({ actor: 'user' }) }),
		);
	});

	it('ignores a cloud workspace event', async () => {
		domainEvents.emit({ type: 'task.created', workspaceId: 42, taskId: 1, task: { id: 1 } });
		await flush();
		expect(invoke).not.toHaveBeenCalled();
	});

	it('ignores an event with no workspace', async () => {
		domainEvents.emit({ type: 'task.created', workspaceId: null, taskId: 1, task: { id: 1 } });
		await flush();
		expect(invoke).not.toHaveBeenCalled();
	});

	it('never forwards timer events, even though PLUGIN_EVENTS grants them a permission', async () => {
		domainEvents.emit({ type: 'timer.started', workspaceId: -3, taskId: 1, task: { id: 1 } });
		await flush();
		expect(invoke).not.toHaveBeenCalled();
	});

	it('ignores events with no plugin permission, such as app.started', async () => {
		domainEvents.emit({ type: 'app.started', workspaceId: -3 } as any);
		await flush();
		expect(invoke).not.toHaveBeenCalled();
	});

	it('strips reaction users and reacted down to emoji and count', async () => {
		domainEvents.emit({
			type: 'comment.reactionChanged',
			workspaceId: -3,
			commentId: 5,
			reactions: [{ emoji: '👍', count: 2, users: [1, 2], reacted: true } as any],
		});
		await flush();
		expect(invoke).toHaveBeenCalledWith(
			'local_access_event',
			expect.objectContaining({ event: expect.objectContaining({ reactions: [{ emoji: '👍', count: 2 }] }) }),
		);
	});

	it('does nothing for a workspace id that resolves to no local workspace', async () => {
		domainEvents.emit({ type: 'task.created', workspaceId: -99, taskId: 1, task: { id: 1 } });
		await flush();
		expect(invoke).not.toHaveBeenCalled();
	});
});
