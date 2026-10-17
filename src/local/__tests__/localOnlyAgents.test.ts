import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { createLocalApi } from '../api';
import {
	handleLocalAccessRequest,
	installLocalAccess,
	type LocalAccessDeps,
	type LocalAccessRequestPayload,
} from '../localAccess';
import { PERSONA_PERMISSIONS } from '../personaGate';
import {
	createLocalPersona,
	ensureDefaultLocalPersona,
	listLocalPersonas,
} from '../personas';
import { migrate } from '../schema';
import type { LocalContext, LocalUser, LocalWorkspace } from '../types';
import { memoryDb, nodeSqliteAvailable } from './nodeDb';

jest.mock('@tauri-apps/api/core', () => ({ invoke: jest.fn() }));
jest.mock('@tauri-apps/api/event', () => ({ listen: jest.fn() }));

const describeSqlite = nodeSqliteAvailable ? describe : describe.skip;

const clock = new Date('2026-10-11T10:00:00Z');
const workspace: LocalWorkspace = {
	id: -1,
	name: 'My workspace',
	code: 'my-workspace',
	schema_version: 0,
	created_at: '',
	path: '/tmp/x',
	database: '/tmp/x/workspace.db',
};
const localUser: LocalUser = { id: -1, name: 'Ada', email: '' };

const makeCtx = async (user: LocalUser): Promise<LocalContext> => {
	const ctx: LocalContext = {
		db: memoryDb(),
		workspace,
		user,
		now: () => clock,
		files: {
			url: (key) => `tmgr://localhost/${key}`,
			read: async () => new Blob(['x']),
			remove: async () => {},
		},
	};
	await migrate(ctx.db, clock.toISOString());
	return ctx;
};

describeSqlite('local-only personas', () => {
	let ctx: LocalContext;

	beforeEach(async () => {
		ctx = await makeCtx(localUser);
	});

	it('creates a persona owned by the local identity and enables it with every permission', async () => {
		const persona = await createLocalPersona(ctx, {
			name: 'Scribe',
			description: 'Writes notes',
		});
		expect(persona.uuid).toMatch(/^[0-9a-f-]{36}$/);

		const rows = await listLocalPersonas(ctx);
		expect(rows).toHaveLength(1);
		expect(rows[0]).toMatchObject({
			uuid: persona.uuid,
			owner_user_id: -1,
			owner_name: 'Ada',
			name: 'Scribe',
			description: 'Writes notes',
			disabled_at: null,
		});
		expect(rows[0].permissions).toEqual([...PERSONA_PERMISSIONS]);
		expect(rows[0].enabled_at).not.toBeNull();
	});

	it('rejects an empty name', async () => {
		await expect(createLocalPersona(ctx, { name: '  ' })).rejects.toThrow();
	});

	it('ensures one default "Local agent" idempotently', async () => {
		const first = await ensureDefaultLocalPersona(ctx);
		const second = await ensureDefaultLocalPersona(ctx);
		expect(second.uuid).toBe(first.uuid);
		const rows = await listLocalPersonas(ctx);
		expect(rows.map((r) => r.name)).toEqual(['Local agent']);
	});

	it('does not add a default when the workspace already has a local persona', async () => {
		await createLocalPersona(ctx, { name: 'Scribe' });
		await ensureDefaultLocalPersona(ctx);
		expect((await listLocalPersonas(ctx)).map((r) => r.name)).toEqual([
			'Scribe',
		]);
	});
});

describeSqlite('socket bridge without a cloud account', () => {
	let ctx: LocalContext;
	let personaUuid: string;

	const depsFor = (user: LocalUser | null): LocalAccessDeps => ({
		resolveWorkspace: async (code) =>
			code === workspace.code ? workspace : null,
		currentUser: () => user,
		contextFor: async (ws, u, actor) => ({
			...ctx,
			workspace: ws,
			user: u,
			actor,
		}),
		tokenInfo: async () => ({
			prefix: 'tmgrl_abcd',
			expiresAt: '2027-01-01T00:00:00Z',
		}),
		promptVersion: async () => null,
	});

	const payload = (
		overrides: Partial<LocalAccessRequestPayload> = {},
	): LocalAccessRequestPayload => ({
		id: 1,
		workspaceCode: workspace.code,
		workspaceId: workspace.id,
		personaUuid,
		personaName: 'Scribe',
		tokenId: 'lt_test',
		method: 'GET',
		path: '/api/local/whoami',
		body: null,
		...overrides,
	});

	const mcp = (body: unknown) =>
		handleLocalAccessRequest(
			payload({ method: 'POST', path: '/api/mcp', body: JSON.stringify(body) }),
			depsFor(localUser),
		);

	beforeEach(async () => {
		ctx = await makeCtx(localUser);
		personaUuid = (await createLocalPersona(ctx, { name: 'Scribe' })).uuid;
	});

	it('answers whoami with user_id -1 and the owner name', async () => {
		const reply = await handleLocalAccessRequest(payload(), depsFor(localUser));
		expect(reply.status).toBe(200);
		const body = JSON.parse(reply.body);
		expect(body.user_id).toBe(-1);
		expect(body.persona.owner).toEqual({ id: -1, name: 'Ada' });
		expect(body.persona.workspace.permissions).toEqual([
			...PERSONA_PERMISSIONS,
		]);
	});

	it('lists MCP tools and creates a task through MCP', async () => {
		const listed = JSON.parse(
			(await mcp({ jsonrpc: '2.0', id: 1, method: 'tools/list' })).body,
		);
		expect(listed.result.tools.map((t: any) => t.name)).toContain(
			'create_task',
		);

		const created = await mcp({
			jsonrpc: '2.0',
			id: 2,
			method: 'tools/call',
			params: { name: 'create_task', arguments: { title: 'From Claude Code' } },
		});
		expect(created.status).toBe(200);
		expect(JSON.parse(created.body).result.isError).toBeFalsy();

		const api = createLocalApi();
		const { dispatchLocal } = await import('../dispatch');
		const tasks = await dispatchLocal(api, ctx, 'GET', 'tasks');
		expect(JSON.stringify(tasks!.data)).toContain('From Claude Code');
	});

	it('still serves a -1 owned persona after the user signs in to the cloud', async () => {
		const cloudUser: LocalUser = {
			id: 42,
			name: 'Ada',
			email: 'ada@example.com',
		};
		const reply = await handleLocalAccessRequest(payload(), depsFor(cloudUser));
		expect(reply.status).toBe(200);
		expect(JSON.parse(reply.body).user_id).toBe(42);
	});

	it('still refuses a cloud persona owned by somebody else', async () => {
		await ctx.db.execute(
			`UPDATE personas SET owner_user_id = 99 WHERE uuid = ?`,
			[personaUuid],
		);
		const reply = await handleLocalAccessRequest(payload(), depsFor(localUser));
		expect(reply.status).toBe(401);
		expect(JSON.parse(reply.body).code).toBe('OWNER_MISMATCH');
	});

	it('refuses a persona owned by a cloud account while in local-only mode', async () => {
		await ctx.db.execute(
			`UPDATE personas SET owner_user_id = 7 WHERE uuid = ?`,
			[personaUuid],
		);
		const reply = await handleLocalAccessRequest(payload(), depsFor(localUser));
		expect(JSON.parse(reply.body).code).toBe('OWNER_MISMATCH');
	});
});

describe('installLocalAccess readiness', () => {
	const invokeMock = invoke as jest.Mock;
	const listenMock = listen as jest.Mock;

	const readyArgs = () =>
		invokeMock.mock.calls
			.filter(([command]) => command === 'local_access_ready')
			.map(([, args]) => args);

	beforeEach(() => {
		invokeMock.mockReset().mockImplementation(async () => undefined);
		listenMock.mockReset().mockResolvedValue(() => {});
	});

	it('reports not ready for an empty user object', async () => {
		await installLocalAccess({
			state: { user: {} },
			getters: {},
			watch: jest.fn(),
		});
		expect(readyArgs()).toEqual([{ userId: null }]);
	});

	it('reports a cloud user id', async () => {
		await installLocalAccess({
			state: { user: { id: 5, name: 'A', email: 'a@b.c' } },
			getters: {},
			watch: jest.fn(),
		});
		expect(readyArgs()).toEqual([{ userId: 5 }]);
	});

	it('reports -1 in local-only mode, from the local profile when state.user is empty', async () => {
		await installLocalAccess({
			state: { user: {}, localProfile: { id: -1, name: 'Ada' } },
			getters: { isLocalOnly: true },
			watch: jest.fn(),
		});
		expect(readyArgs()).toEqual([{ userId: -1 }]);
	});

	it('reports -1 in local-only mode from the local user identity', async () => {
		await installLocalAccess({
			state: { user: { id: -1, name: 'Ada', email: '', is_local: true } },
			getters: { isLocalOnly: true },
			watch: jest.fn(),
		});
		expect(readyArgs()).toEqual([{ userId: -1 }]);
	});
});
