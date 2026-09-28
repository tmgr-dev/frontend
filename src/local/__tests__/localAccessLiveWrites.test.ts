import { domainEvents, type DomainEvent } from '@/utils/domainEvents';
import type { EventHandlers } from '@/types/dashboard';
import { createLocalApi } from '../api';
import { dispatchLocal } from '../dispatch';
import { installLocalLiveUpdates } from '../liveUpdates';
import { handleLocalAccessRequest, type LocalAccessDeps, type LocalAccessRequestPayload } from '../localAccess';
import { enableLocalPersona } from '../personas';
import { migrate } from '../schema';
import type { LocalContext, LocalUser, LocalWorkspace } from '../types';
import { memoryDb, nodeSqliteAvailable } from './nodeDb';

jest.mock('@tauri-apps/api/core', () => ({ invoke: jest.fn() }));
jest.mock('@tauri-apps/api/event', () => ({ listen: jest.fn() }));

const describeSqlite = nodeSqliteAvailable ? describe : describe.skip;

describeSqlite('persona writes over the local socket reach the open UI', () => {
	let ctx: LocalContext;
	let taskId: number;
	let events: DomainEvent[];
	let unsubscribe: () => void;
	const clock = new Date('2026-09-28T10:00:00Z');
	const workspace: LocalWorkspace = {
		id: -1,
		name: 'Personal',
		code: 'local-personal',
		schema_version: 0,
		created_at: '',
		path: '/tmp/x',
		database: '/tmp/x/workspace.db',
	};
	const user: LocalUser = { id: 7, name: 'Yurij', email: 'me@example.com' };
	const deps: LocalAccessDeps = {
		resolveWorkspace: async (code) => (code === workspace.code ? workspace : null),
		currentUser: () => user,
		contextFor: async (ws, u, actor) => ({ ...ctx, workspace: ws, user: u, actor }),
		tokenInfo: async () => null,
		promptVersion: async () => null,
	};
	const request = (method: string, path: string, body?: unknown): LocalAccessRequestPayload => ({
		id: 1,
		workspaceCode: workspace.code,
		workspaceId: workspace.id,
		personaUuid: 'p-1',
		personaName: 'Reviewer',
		tokenId: 'lt_test',
		method,
		path,
		body: body === undefined ? null : JSON.stringify(body),
	});
	const mcpCall = (name: string, args: Record<string, unknown>) =>
		request('POST', '/mcp', { jsonrpc: '2.0', id: 1, method: 'tools/call', params: { name, arguments: args } });

	beforeEach(async () => {
		ctx = {
			db: memoryDb(),
			workspace,
			user,
			now: () => clock,
			files: {
				url: (key) => `tmgrfile://localhost/${key}`,
				read: async () => new Blob(['x']),
				remove: async () => {},
			},
		};
		await migrate(ctx.db, clock.toISOString());
		await ctx.db.execute(
			`INSERT INTO personas (uuid, owner_user_id, owner_name, name, description, avatar_file, synced_at, archived_at)
			 VALUES ('p-1', 7, 'Yurij', 'Reviewer', NULL, NULL, ?, NULL)`,
			[clock.toISOString()],
		);
		await enableLocalPersona(ctx, 'p-1', ['tasks:read', 'tasks:write', 'comments:write', 'agent_work:write']);
		const created = await dispatchLocal(createLocalApi(), ctx, 'POST', 'tasks', { title: 'Review the PR' });
		taskId = created!.data.data.id;
		events = [];
		unsubscribe = domainEvents.on((event) => events.push(event));
	});

	afterEach(() => unsubscribe());

	it('emits comment.created as the persona for an MCP add_comment', async () => {
		const reply = await handleLocalAccessRequest(mcpCall('add_comment', { taskId, message: 'from mcp' }), deps);
		expect(JSON.parse(reply.body).result.isError).toBeFalsy();
		expect(events).toEqual([
			expect.objectContaining({ type: 'comment.created', workspaceId: -1, taskId, actor: 'persona:p-1' }),
		]);
	});

	it('emits task.updated and task.statusChanged for an MCP update_task with a status', async () => {
		await handleLocalAccessRequest(mcpCall('update_task', { taskId, title: 'Renamed', statusId: 2 }), deps);
		expect(events.map((e) => [e.type, e.actor])).toEqual([
			['task.updated', 'persona:p-1'],
			['task.statusChanged', 'persona:p-1'],
		]);
	});

	it('emits a REST write exactly once', async () => {
		await handleLocalAccessRequest(request('POST', `/api/tasks/${taskId}/comments`, { message: 'hi' }), deps);
		expect(events.filter((e) => e.type === 'comment.created')).toHaveLength(1);
	});

	it('hands agent work runs from REST and MCP to onAgentWorkChanged', async () => {
		const onAgentWorkChanged = jest.fn();
		const uninstall = installLocalLiveUpdates({
			deliver: (workspaceId, call) => {
				if (workspaceId === -1) call({ onAgentWorkChanged } as EventHandlers);
			},
			fetchTask: async () => null,
			invalidate: () => {},
		});
		const started = await handleLocalAccessRequest(
			request('POST', `/api/tasks/${taskId}/agent-work`, { agent: 'claude' }),
			deps,
		);
		const runId = JSON.parse(started.body).data.id;
		await handleLocalAccessRequest(mcpCall('finish_agent_work', { runId, status: 'succeeded' }), deps);
		uninstall();
		expect(onAgentWorkChanged.mock.calls.map(([run]) => [run.id, run.task_id, run.status])).toEqual([
			[runId, taskId, 'running'],
			[runId, taskId, 'succeeded'],
		]);
	});
});
