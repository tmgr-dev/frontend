import type { EventHandlers } from '@/types/dashboard';
import { createDomainEvents, domainEvents } from '@/utils/domainEvents';
import { createLocalApi } from '../api';
import { dispatchLocal } from '../dispatch';
import { installLocalLiveUpdates } from '../liveUpdates';
import {
	handleLocalAccessRequest,
	installLocalAccessEvents,
	type LocalAccessDeps,
	type LocalAccessRequestPayload,
} from '../localAccess';
import { enableLocalPersona } from '../personas';
import { listLocalWorkspaces } from '../runtime';
import { migrate } from '../schema';
import type { LocalContext, LocalUser, LocalWorkspace } from '../types';
import { memoryDb, nodeSqliteAvailable } from './nodeDb';

jest.mock('@tauri-apps/api/core', () => ({ invoke: jest.fn() }));
jest.mock('@tauri-apps/api/event', () => ({ listen: jest.fn() }));
jest.mock('../runtime', () => ({ listLocalWorkspaces: jest.fn() }));

const describeSqlite = nodeSqliteAvailable ? describe : describe.skip;
const flush = () => new Promise((resolve) => setTimeout(resolve, 0));

describeSqlite('pages over the local access socket', () => {
	let ctx: LocalContext;
	let pageId: number;
	const clock = new Date('2026-10-01T10:00:00Z');
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
	const deps = (): LocalAccessDeps => ({
		resolveWorkspace: async () => workspace,
		currentUser: () => user,
		contextFor: async (ws, u, actor) => ({
			...ctx,
			workspace: ws,
			user: u,
			actor,
		}),
		tokenInfo: async () => null,
		promptVersion: async () => null,
	});
	const request = (
		overrides: Partial<LocalAccessRequestPayload>,
	): LocalAccessRequestPayload => ({
		id: 1,
		workspaceCode: workspace.code,
		workspaceId: workspace.id,
		personaUuid: 'p-1',
		personaName: 'Analyst',
		tokenId: 'lt_1',
		method: 'GET',
		path: '/api/pages',
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
			 VALUES ('p-1', 7, 'Yurij', 'Analyst', NULL, NULL, ?, NULL)`,
			[clock.toISOString()],
		);
		await enableLocalPersona(ctx, 'p-1', ['pages:read', 'pages:write']);
		const created = await dispatchLocal(
			createLocalApi(),
			ctx,
			'POST',
			'pages',
			{ title: 'Doc', body: 'one' },
		);
		pageId = created!.data.data.id;
	});

	it('serves page reads and writes as the persona and tags the event with it', async () => {
		const events: any[] = [];
		const off = domainEvents.on(
			(event) => event.type.startsWith('page.') && events.push(event),
		);
		const reply = await handleLocalAccessRequest(
			request({
				method: 'POST',
				path: `/api/pages/${pageId}/append`,
				body: JSON.stringify({ markdown: 'two', summary: 'note' }),
			}),
			deps(),
		);
		off();
		expect(reply.status).toBe(200);
		expect(JSON.parse(reply.body).data).toMatchObject({
			version: 2,
			updated_by: { kind: 'persona', id: 'p-1' },
		});
		expect(events).toEqual([
			expect.objectContaining({
				type: 'page.updated',
				workspaceId: -1,
				actor: 'persona:p-1',
				page: expect.objectContaining({
					id: pageId,
					version: 2,
					summary: 'note',
				}),
			}),
		]);
	});

	it('keeps the error code and the current page in a 409 body', async () => {
		await handleLocalAccessRequest(
			request({
				method: 'PATCH',
				path: `/api/pages/${pageId}`,
				body: JSON.stringify({ version: 1, body: 'two' }),
			}),
			deps(),
		);
		const stale = await handleLocalAccessRequest(
			request({
				method: 'PATCH',
				path: `/api/pages/${pageId}`,
				body: JSON.stringify({ version: 1, body: 'mine' }),
			}),
			deps(),
		);
		expect(stale.status).toBe(409);
		expect(JSON.parse(stale.body)).toMatchObject({
			error: 'page_conflict',
			code: 'CONFLICT',
			message: 'Page was changed',
			data: { id: pageId, version: 2, body: 'two' },
		});
	});

	it('keeps field errors of invalid properties and section refusals', async () => {
		const invalid = await handleLocalAccessRequest(
			request({
				method: 'POST',
				path: '/api/pages',
				body: JSON.stringify({
					title: 'P',
					type: 'person',
					properties: { network: 'x' },
				}),
			}),
			deps(),
		);
		expect(invalid.status).toBe(422);
		expect(JSON.parse(invalid.body)).toMatchObject({
			error: 'invalid_properties',
			errors: { network: expect.any(String) },
		});
		const forbidden = await handleLocalAccessRequest(
			request({
				method: 'POST',
				path: '/api/pages',
				body: JSON.stringify({ title: 'C', type: 'context' }),
			}),
			deps(),
		);
		expect([forbidden.status, JSON.parse(forbidden.body).error]).toEqual([
			403,
			'forbidden',
		]);
	});

	it('refuses a persona that was not granted pages access', async () => {
		await enableLocalPersona(ctx, 'p-1', ['tasks:read']);
		const reply = await handleLocalAccessRequest(
			request({ path: '/api/pages/tree' }),
			deps(),
		);
		expect([reply.status, JSON.parse(reply.body).code]).toEqual([
			403,
			'PERMISSION_MISSING',
		]);
	});

	it('answers the pages tools through /mcp', async () => {
		const reply = await handleLocalAccessRequest(
			request({
				method: 'POST',
				path: '/mcp',
				body: JSON.stringify({
					jsonrpc: '2.0',
					id: 1,
					method: 'tools/call',
					params: { name: 'pages_get', arguments: { id: String(pageId) } },
				}),
			}),
			deps(),
		);
		const text = JSON.parse(JSON.parse(reply.body).result.content[0].text);
		expect(text).toMatchObject({ id: pageId, body: 'one' });
	});
});

describe('page events on the local access stream', () => {
	const workspace: LocalWorkspace = {
		id: -3,
		name: 'P',
		code: 'local-p',
		schema_version: 0,
		created_at: '',
		path: '/tmp/x',
		database: '/tmp/x/workspace.db',
	};
	let invoke: jest.Mock;
	let unsubscribe: () => void;
	beforeEach(() => {
		invoke = jest.fn().mockResolvedValue(undefined);
		(listLocalWorkspaces as jest.Mock).mockResolvedValue([workspace]);
		unsubscribe = installLocalAccessEvents(invoke);
	});
	afterEach(() => unsubscribe());

	it('forwards every page event gated by pages:read, with the actor', async () => {
		const page = {
			id: 4,
			slug: 'doc',
			title: 'Doc',
			type: 'plain',
			parent_id: null,
			version: 2,
			updated_by: { kind: 'user' },
			summary: null,
			linked_task_ids: [],
		};
		for (const type of [
			'page.created',
			'page.updated',
			'page.deleted',
			'page.restored',
			'page.moved',
		] as const) {
			domainEvents.emit({ type, workspaceId: -3, pageId: 4, page, actor: 'persona:p-1' });
		}
		await flush();
		expect(invoke).toHaveBeenCalledTimes(5);
		expect(invoke).toHaveBeenCalledWith('local_access_event', {
			workspaceCode: 'local-p',
			permission: 'pages:read',
			event: expect.objectContaining({
				type: 'page.moved',
				page,
				actor: 'persona:p-1',
			}),
		});
	});

	it('defaults the actor to user and ignores cloud workspaces', async () => {
		domainEvents.emit({
			type: 'page.updated',
			workspaceId: -3,
			pageId: 1,
			page: { id: 1 },
		});
		domainEvents.emit({
			type: 'page.updated',
			workspaceId: 12,
			pageId: 1,
			page: { id: 1 },
		});
		await flush();
		expect(invoke).toHaveBeenCalledTimes(1);
		expect(invoke.mock.calls[0][1].event.actor).toBe('user');
	});
});

describe('page events and the open UI', () => {
	it('refreshes through the cloud realtime handler for non-UI writes only', () => {
		const bus = createDomainEvents();
		const onPageEvent = jest.fn();
		const deps = {
			deliver: jest.fn(
				(workspaceId: number, call: (h: EventHandlers) => void) =>
					workspaceId === -3 && call({ onPageEvent }),
			),
			fetchTask: jest.fn(),
			invalidate: jest.fn(),
		};
		const off = installLocalLiveUpdates(deps, bus);
		const page = {
			id: 4,
			slug: 'doc',
			title: 'Doc',
			type: 'plain',
			parent_id: null,
			version: 2,
			updated_by: {},
			summary: null,
			linked_task_ids: [9],
		};
		bus.emit({
			type: 'page.updated',
			workspaceId: -3,
			pageId: 4,
			page,
			actor: 'persona:p-1',
		});
		bus.emit({
			type: 'page.created',
			workspaceId: -3,
			pageId: 4,
			page,
			actor: 'plugin:x',
		});
		bus.emit({ type: 'page.updated', workspaceId: -3, pageId: 4, page });
		bus.emit({
			type: 'page.updated',
			workspaceId: 5,
			pageId: 4,
			page,
			actor: 'persona:p-1',
		});
		off();
		expect(onPageEvent).toHaveBeenCalledTimes(2);
		expect(onPageEvent).toHaveBeenNthCalledWith(1, 'page.updated', { page });
		expect(onPageEvent).toHaveBeenNthCalledWith(2, 'page.created', { page });
		expect(deps.invalidate).toHaveBeenCalledWith(/^pages-/);
	});
});
