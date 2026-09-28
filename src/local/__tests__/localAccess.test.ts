import { domainEvents } from '@/utils/domainEvents';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { createLocalApi } from '../api';
import { dispatchLocal } from '../dispatch';
import {
	handleLocalAccessRequest,
	installLocalAccess,
	type LocalAccessDeps,
	type LocalAccessRequestPayload,
} from '../localAccess';
import { disableLocalPersona, enableLocalPersona } from '../personas';
import { migrate } from '../schema';
import type { LocalContext, LocalUser, LocalWorkspace } from '../types';
import { memoryDb, nodeSqliteAvailable } from './nodeDb';

jest.mock('@tauri-apps/api/core', () => ({ invoke: jest.fn() }));
jest.mock('@tauri-apps/api/event', () => ({ listen: jest.fn() }));

const describeSqlite = nodeSqliteAvailable ? describe : describe.skip;

describeSqlite('local access socket bridge handler', () => {
	let ctx: LocalContext;
	let taskId: number;
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

	const baseDeps = (): LocalAccessDeps => ({
		resolveWorkspace: async (code) => (code === workspace.code ? workspace : null),
		currentUser: () => user,
		contextFor: async (ws, u, actor) => ({ ...ctx, workspace: ws, user: u, actor }),
		tokenInfo: async () => ({ prefix: 'tmgrl_abcd', expiresAt: '2027-01-01T00:00:00Z' }),
		promptVersion: async () => 3,
	});

	const payload = (overrides: Partial<LocalAccessRequestPayload> = {}): LocalAccessRequestPayload => ({
		id: 1,
		workspaceCode: workspace.code,
		workspaceId: workspace.id,
		personaUuid: 'p-1',
		personaName: 'Reviewer',
		tokenId: 'lt_test',
		method: 'GET',
		path: `/api/tasks/${taskId}`,
		body: null,
		...overrides,
	});

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
		await enableLocalPersona(ctx, 'p-1', ['tasks:read', 'tasks:write', 'comments:write']);
		const api = createLocalApi();
		const created = await dispatchLocal(api, ctx, 'POST', 'tasks', { title: 'Review the PR' });
		taskId = created!.data.data.id;
	});

	it('writes a comment through the handler with author.kind persona and emits the domain event', async () => {
		const events: any[] = [];
		const unsubscribe = domainEvents.on((event) => events.push(event));
		const reply = await handleLocalAccessRequest(
			payload({
				method: 'POST',
				path: `/api/tasks/${taskId}/comments`,
				body: JSON.stringify({ message: 'hi' }),
			}),
			baseDeps(),
		);
		unsubscribe();
		expect(reply.status).toBe(201);
		const body = JSON.parse(reply.body);
		expect(body.data.author).toMatchObject({ kind: 'persona', id: 'p-1', name: 'Reviewer' });
		expect(events).toContainEqual(expect.objectContaining({ type: 'comment.created', actor: 'persona:p-1' }));
	});

	it('ignores an actor smuggled in the request body: there is no header path to become "user"', async () => {
		const reply = await handleLocalAccessRequest(
			payload({
				method: 'POST',
				path: `/api/tasks/${taskId}/comments`,
				body: JSON.stringify({ message: 'hi', actor: { kind: 'user', id: '7', name: 'Yurij' } }),
			}),
			baseDeps(),
		);
		const body = JSON.parse(reply.body);
		expect(body.data.author.kind).toBe('persona');
		expect(body.data.author.id).toBe('p-1');
	});

	it('refuses a cloud workspace without ever reaching the router', async () => {
		const deps = baseDeps();
		let resolved = false;
		let contextBuilt = false;
		deps.resolveWorkspace = async () => {
			resolved = true;
			return { ...workspace, id: 42 };
		};
		deps.contextFor = async (...args) => {
			contextBuilt = true;
			return baseDeps().contextFor(...args);
		};
		const reply = await handleLocalAccessRequest(payload(), deps);
		expect(resolved).toBe(true);
		expect(reply.status).toBe(403);
		expect(JSON.parse(reply.body).code).toBe('WORKSPACE_NOT_LOCAL');
		expect(contextBuilt).toBe(false);
	});

	it('refuses a gone workspace with 409 WORKSPACE_GONE', async () => {
		const deps = baseDeps();
		deps.resolveWorkspace = async () => null;
		const reply = await handleLocalAccessRequest(payload(), deps);
		expect(reply.status).toBe(409);
		expect(JSON.parse(reply.body).code).toBe('WORKSPACE_GONE');
	});

	it('refuses a workspace_id that does not match the token workspace', async () => {
		const reply = await handleLocalAccessRequest(
			payload({ path: `/api/tasks/${taskId}?workspace_id=-99` }),
			baseDeps(),
		);
		expect(reply.status).toBe(403);
		expect(JSON.parse(reply.body).code).toBe('WORKSPACE_MISMATCH');
	});

	it('refuses a route outside the persona whitelist', async () => {
		const reply = await handleLocalAccessRequest(
			payload({ method: 'DELETE', path: `/api/tasks/${taskId}` }),
			baseDeps(),
		);
		expect(reply.status).toBe(403);
		expect(JSON.parse(reply.body).code).toBe('ROUTE_NOT_ALLOWED');
	});

	it('refuses a disabled persona with 401 PERSONA_DISABLED', async () => {
		await disableLocalPersona(ctx, 'p-1');
		const reply = await handleLocalAccessRequest(payload(), baseDeps());
		expect(reply.status).toBe(401);
		expect(JSON.parse(reply.body).code).toBe('PERSONA_DISABLED');
	});

	it('refuses a persona owned by a different account with 401 OWNER_MISMATCH', async () => {
		await ctx.db.execute(`UPDATE personas SET owner_user_id = 99 WHERE uuid = 'p-1'`);
		const reply = await handleLocalAccessRequest(payload(), baseDeps());
		expect(reply.status).toBe(401);
		expect(JSON.parse(reply.body).code).toBe('OWNER_MISMATCH');
	});

	it('answers an unknown route with 501 NOT_AVAILABLE_LOCALLY', async () => {
		const reply = await handleLocalAccessRequest(payload({ path: '/mcp' }), baseDeps());
		expect(reply.status).toBe(501);
		expect(JSON.parse(reply.body).code).toBe('NOT_AVAILABLE_LOCALLY');
	});

	it('answers an unlisted /api/local/* route with 404', async () => {
		const reply = await handleLocalAccessRequest(payload({ path: '/api/local/events' }), baseDeps());
		expect(reply.status).toBe(404);
		expect(JSON.parse(reply.body).code).toBe('NOT_FOUND');
	});

	it('answers GET /api/local/whoami', async () => {
		const reply = await handleLocalAccessRequest(payload({ path: '/api/local/whoami' }), baseDeps());
		expect(reply.status).toBe(200);
		const body = JSON.parse(reply.body);
		expect(body.user_id).toBe(7);
		expect(body.persona).toMatchObject({
			id: 'p-1',
			name: 'Reviewer',
			owner: { id: 7, name: 'Yurij' },
			prompt_version: 3,
			workspace: { id: -1, code: 'local-personal' },
			skills: [],
		});
		expect(body.persona.workspace.permissions).toEqual(expect.arrayContaining(['tasks:read', 'tasks:write']));
		expect(body.token).toMatchObject({ id: 'lt_test', prefix: 'tmgrl_abcd' });
	});

	it('falls through to 501 for a route the local router has never heard of', async () => {
		const reply = await handleLocalAccessRequest(payload({ path: '/api/definitely-not-a-route' }), baseDeps());
		expect(reply.status).toBe(501);
		expect(JSON.parse(reply.body).code).toBe('NOT_AVAILABLE_LOCALLY');
	});
});

describe('installLocalAccess wiring', () => {
	const invokeMock = invoke as jest.Mock;
	const listenMock = listen as jest.Mock;

	beforeEach(() => {
		invokeMock.mockReset().mockImplementation(async (command: string) => (command === 'local_workspaces_list' ? [] : undefined));
		listenMock.mockReset();
	});

	it('registers the request handler before announcing readiness, and replies with the request id', async () => {
		let handler: ((event: { payload: LocalAccessRequestPayload }) => void) | undefined;
		listenMock.mockImplementation((_channel: string, callback: typeof handler) => {
			handler = callback;
			return Promise.resolve(() => {});
		});
		const store = { state: { user: null }, watch: jest.fn() };

		await installLocalAccess(store);

		expect(listenMock).toHaveBeenCalledWith('local-access://request', expect.any(Function));
		const listenOrder = listenMock.mock.invocationCallOrder[0];
		const readyCall = invokeMock.mock.calls.findIndex(([command]) => command === 'local_access_ready');
		expect(readyCall).toBeGreaterThanOrEqual(0);
		expect(invokeMock.mock.invocationCallOrder[readyCall]).toBeGreaterThan(listenOrder);
		expect(store.watch).toHaveBeenCalled();

		invokeMock.mockClear();
		handler!({
			payload: {
				id: 42,
				workspaceCode: 'nope',
				workspaceId: -1,
				personaUuid: 'p-1',
				personaName: 'Reviewer',
				tokenId: 'lt_x',
				method: 'GET',
				path: '/api/tasks/1',
				body: null,
			},
		});
		await new Promise((resolve) => setTimeout(resolve, 0));
		expect(invokeMock).toHaveBeenCalledWith(
			'local_access_reply',
			expect.objectContaining({ id: 42, status: 409 }),
		);
	});
});
