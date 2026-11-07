import { createLocalApi } from '../api';
import { dispatchLocal } from '../dispatch';
import { handleLocalAccessRequest, type LocalAccessDeps } from '../localAccess';
import { handleMcpRequest } from '../mcp';
import { readInventory } from '../moveToCloud/inventory';
import { isLinkedLocalOwner } from '../personaGate';
import { enableLocalPersona } from '../personas';
import { migrate } from '../schema';
import type { LocalContext, LocalUser, LocalWorkspace } from '../types';
import { memoryDb, nodeSqliteAvailable } from './nodeDb';

const describeSqlite = nodeSqliteAvailable ? describe : describe.skip;

describe('isLinkedLocalOwner', () => {
	it('maps only a positive user equal to a numeric linked id', () => {
		expect(isLinkedLocalOwner(7, 7)).toBe(true);
		expect(isLinkedLocalOwner(7, '7')).toBe(true);
		expect(isLinkedLocalOwner(8, 7)).toBe(false);
		expect(isLinkedLocalOwner(7, undefined)).toBe(false);
		expect(isLinkedLocalOwner(7, null)).toBe(false);
		expect(isLinkedLocalOwner(7, 'abc')).toBe(false);
		expect(isLinkedLocalOwner(7, '')).toBe(false);
		expect(isLinkedLocalOwner(-1, -1)).toBe(false);
		expect(isLinkedLocalOwner(0, 0)).toBe(false);
	});
});

describeSqlite('stored -1 identity read per context', () => {
	const clock = new Date('2026-11-02T10:00:00Z');
	const workspace: LocalWorkspace = {
		id: -1,
		name: 'Personal',
		code: 'local-personal',
		schema_version: 0,
		created_at: '',
		path: '/tmp/x',
		database: '/tmp/x/workspace.db',
	};
	const router = createLocalApi();
	let base: LocalContext;
	let taskId: number;
	let commentId: number;

	const as = (
		user: LocalUser,
		extra: Partial<LocalContext> = {},
	): LocalContext => ({ ...base, user, ...extra });
	const linked = () =>
		as(
			{ id: 7, name: 'Yurij', email: '' },
			{
				localOwnerLinked: true,
			},
		);
	const accountB = () => as({ id: 8, name: 'Bella', email: '' });
	const missingField = () => as({ id: 7, name: 'Yurij', email: '' });
	const localMode = () =>
		as(
			{ id: -1, name: 'Local', email: '' },
			{
				accountless: true,
			},
		);

	const api = async (
		ctx: LocalContext,
		method: string,
		url: string,
		body?: unknown,
	) => (await dispatchLocal(router, ctx, method, url, body))!.data.data;

	beforeEach(async () => {
		base = {
			db: memoryDb(),
			workspace,
			user: { id: -1, name: 'Local', email: '' },
			now: () => clock,
			files: {
				url: (key) => `tmgr://x/${key}`,
				read: async () => new Blob(['x']),
				remove: async () => {},
			},
		};
		await migrate(base.db, clock.toISOString());
		const iso = clock.toISOString();
		taskId = (await api(localMode(), 'POST', 'tasks', { title: 'T' })).id;
		const comment = await base.db.execute(
			`INSERT INTO comments (task_id, message, created_at, updated_at, author_kind, author_id, author_name)
			 VALUES (?, 'old', ?, ?, 'user', '-1', 'Local')`,
			[taskId, iso, iso],
		);
		commentId = Number(comment.lastInsertId);
		await base.db.execute(
			`INSERT INTO comment_reactions (comment_id, emoji, actor_kind, actor_id, created_at) VALUES (?, '👍', 'user', '-1', ?)`,
			[commentId, iso],
		);
		await base.db.execute(
			`INSERT INTO agent_work_runs (task_id, agent, status, started_at, created_at, updated_at, actor_kind, actor_id)
			 VALUES (?, 'claude-code', 'running', ?, ?, ?, 'user', '-1')`,
			[taskId, iso, iso, iso],
		);
		await base.db.execute(
			`INSERT INTO personas (uuid, owner_user_id, owner_name, name, description, avatar_file, synced_at, archived_at)
			 VALUES ('p-local', -1, 'Local', 'Local agent', NULL, NULL, ?, NULL)`,
			[iso],
		);
		await enableLocalPersona(base, 'p-local', ['tasks:read', 'comments:write']);
	});

	const authorOf = async (ctx: LocalContext) =>
		(await api(ctx, 'GET', `tasks/${taskId}/comments`)).find(
			(c: any) => c.id === commentId,
		).author;

	it('reads the comment author as the linked user only', async () => {
		expect(await authorOf(linked())).toEqual({
			kind: 'user',
			id: '7',
			name: 'Yurij',
		});
		for (const ctx of [accountB(), missingField(), localMode()])
			expect(await authorOf(ctx)).toEqual({
				kind: 'user',
				id: '-1',
				name: 'Local',
			});
	});

	const actorOfRun = async (ctx: LocalContext) =>
		(await api(ctx, 'GET', `tasks/${taskId}/agent-work`)).runs[0].actor;

	it('reads the agent-work actor as the linked user only', async () => {
		expect((await actorOfRun(linked())).id).toBe('7');
		for (const ctx of [accountB(), missingField(), localMode()])
			expect((await actorOfRun(ctx)).id).toBe('-1');
	});

	const reactionsOf = async (ctx: LocalContext) =>
		(await api(ctx, 'GET', `tasks/${taskId}/comments`)).find(
			(c: any) => c.id === commentId,
		).reactions;

	it('shows the -1 reaction as reacted for the linked user only', async () => {
		expect(await reactionsOf(linked())).toEqual([
			{
				emoji: '👍',
				count: 1,
				reacted: true,
				users: [{ id: 7, name: 'Yurij' }],
			},
		]);
		for (const ctx of [accountB(), missingField()])
			expect((await reactionsOf(ctx))[0]).toMatchObject({
				reacted: false,
				users: [{ id: -1, name: ctx.user.name }],
			});
		expect((await reactionsOf(localMode()))[0]).toMatchObject({
			reacted: true,
		});
	});

	it('counts and lists the person once when both the -1 row and the linked row exist', async () => {
		await base.db.execute(
			`INSERT INTO comment_reactions (comment_id, emoji, actor_kind, actor_id, created_at) VALUES (?, '👍', 'user', '7', ?)`,
			[commentId, clock.toISOString()],
		);
		expect(await reactionsOf(linked())).toEqual([
			{
				emoji: '👍',
				count: 1,
				reacted: true,
				users: [{ id: 7, name: 'Yurij' }],
			},
		]);
		expect((await reactionsOf(missingField()))[0]).toMatchObject({
			count: 2,
		});
	});

	const toggle = (ctx: LocalContext) =>
		api(ctx, 'POST', `comments/${commentId}/reactions/toggle`, { emoji: '👍' });
	const stored = async () =>
		(
			await base.db.select<any>(
				`SELECT actor_id FROM comment_reactions ORDER BY actor_id`,
			)
		).map((r) => String(r.actor_id));

	it('removes the -1 reaction on the linked second click instead of duplicating', async () => {
		const result = await toggle(linked());
		expect(result.reactions).toEqual([]);
		expect(await stored()).toEqual([]);
		const again = await toggle(linked());
		expect(again.reactions[0]).toMatchObject({ count: 1, reacted: true });
		expect(await stored()).toEqual(['7']);
	});

	it('leaves the -1 reaction alone when another account toggles', async () => {
		const result = await toggle(accountB());
		expect(result.reactions[0]).toMatchObject({ count: 2, reacted: true });
		expect(await stored()).toEqual(['-1', '8']);
		await toggle(missingField());
		expect(await stored()).toEqual(['-1', '7', '8']);
	});

	it('keeps -1 as the stored value after every read and write', async () => {
		await authorOf(linked());
		await actorOfRun(linked());
		await reactionsOf(linked());
		const [c] = await base.db.select<any>(`SELECT author_id FROM comments`);
		const [r] = await base.db.select<any>(
			`SELECT actor_id FROM agent_work_runs`,
		);
		const [p] = await base.db.select<any>(`SELECT owner_user_id FROM personas`);
		expect([String(c.author_id), String(r.actor_id), p.owner_user_id]).toEqual([
			'-1',
			'-1',
			-1,
		]);
	});

	const ownerOnTask = async (ctx: LocalContext) => {
		await api(ctx, 'POST', `tasks/${taskId}/personas/p-local`);
		const task = await api(ctx, 'GET', `tasks/${taskId}`);
		return task.persona_assignees[0].owner;
	};

	it('shows the persona owner in the task view as the linked user only', async () => {
		expect(await ownerOnTask(linked())).toEqual({ id: 7, name: 'Yurij' });
	});

	it.each([
		['account B', () => accountB],
		['missing linked id', () => missingField],
	])('keeps the persona owner -1 for %s', async (_name, pick) => {
		expect(await ownerOnTask(pick()())).toEqual({ id: -1, name: 'Local' });
	});

	it('keeps the persona owner -1 in local mode', async () => {
		expect(await ownerOnTask(localMode())).toEqual({ id: -1, name: 'Local' });
	});

	it('lists assignable personas with the mapped owner for the linked user only', async () => {
		const list = async (ctx: LocalContext) =>
			(
				await api(ctx, 'GET', `workspaces/${workspace.id}/assignable-personas`)
			).map((p: any) => p.owner);
		expect(await list(linked())).toEqual([{ id: 7, name: 'Yurij' }]);
		expect(await list(accountB())).toEqual([{ id: -1, name: 'Local' }]);
	});

	const whoamiMcp = async (ctx: LocalContext) => {
		const res = await handleMcpRequest(
			router,
			{
				...ctx,
				actor: { kind: 'persona', id: 'p-local', name: 'Local agent' },
			},
			JSON.stringify({
				jsonrpc: '2.0',
				id: 1,
				method: 'tools/call',
				params: { name: 'whoami', arguments: {} },
			}),
			{ personaPrompt: async () => null },
		);
		return JSON.parse(JSON.parse(res.body).result.content[0].text).persona
			.owner;
	};

	it('answers mcp whoami with the mapped owner for the linked user only', async () => {
		expect(await whoamiMcp(linked())).toEqual({ id: 7, name: 'Yurij' });
		expect(await whoamiMcp(accountB())).toEqual({ id: -1, name: 'Local' });
		expect(await whoamiMcp(missingField())).toEqual({ id: -1, name: 'Local' });
	});

	const whoamiAccess = async (ctx: LocalContext) => {
		const deps: LocalAccessDeps = {
			resolveWorkspace: async () => workspace,
			currentUser: () => ctx.user,
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
			promptVersion: async () => 1,
		};
		const reply = await handleLocalAccessRequest(
			{
				id: 1,
				workspaceCode: workspace.code,
				workspaceId: workspace.id,
				personaUuid: 'p-local',
				personaName: 'Local agent',
				tokenId: 'lt',
				method: 'GET',
				path: '/api/local/whoami',
				body: null,
			},
			deps,
		);
		return JSON.parse(reply.body).persona.owner;
	};

	it('answers the local access whoami with the mapped owner for the linked user only', async () => {
		expect(await whoamiAccess(linked())).toEqual({ id: 7, name: 'Yurij' });
		expect(await whoamiAccess(accountB())).toEqual({ id: -1, name: 'Local' });
		expect(await whoamiAccess(missingField())).toEqual({
			id: -1,
			name: 'Local',
		});
	});

	it('keeps the move-to-cloud inventory on the raw -1 owner', async () => {
		await ownerOnTask(linked());
		const { personas } = await readInventory(base.db);
		expect(personas).toEqual([
			expect.objectContaining({ uuid: 'p-local', ownerUserId: -1 }),
		]);
	});
});
