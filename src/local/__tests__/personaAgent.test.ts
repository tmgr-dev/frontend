import { createLocalApi } from '../api';
import { dispatchLocal } from '../dispatch';
import type { ChatStreamEvent } from '../personaAgent';
import { runPersonaAgent } from '../personaAgent';
import { enableLocalPersona } from '../personas';
import { migrate } from '../schema';
import type { LocalContext } from '../types';
import { memoryDb, nodeSqliteAvailable } from './nodeDb';

const describeSqlite = nodeSqliteAvailable ? describe : describe.skip;

describeSqlite('local persona agent loop', () => {
	let ctx: LocalContext;
	let taskId: number;
	const clock = new Date('2026-09-27T12:00:00Z');
	const router = createLocalApi();
	const persona = { uuid: 'p-1', name: 'Reviewer' };

	beforeEach(async () => {
		ctx = {
			db: memoryDb(),
			workspace: { id: -1, name: 'Personal', code: 'local-personal', schema_version: 0, created_at: '', path: '/tmp/x', database: '/tmp/x/workspace.db' },
			user: { id: 7, name: 'Yurij', email: 'me@example.com' },
			now: () => clock,
			files: { url: (key) => `tmgrfile://localhost/${key}`, read: async () => new Blob(['x']), remove: async () => {} },
		};
		await migrate(ctx.db, clock.toISOString());
		await ctx.db.execute(
			`INSERT INTO personas (uuid, owner_user_id, owner_name, name, description, avatar_file, synced_at, archived_at)
			 VALUES ('p-1', 7, 'Yurij', 'Reviewer', NULL, NULL, ?, NULL)`,
			[clock.toISOString()],
		);
		await enableLocalPersona(ctx, 'p-1', ['tasks:read', 'comments:write', 'agent_work:read', 'agent_work:write']);
		const created = await dispatchLocal(router, ctx, 'POST', 'tasks', { title: 'Review the PR' });
		taskId = created!.data.data.id;
	});

	it('calls one tool, then answers with text, and records the run for the persona', async () => {
		let calls = 0;
		const chat = async function* (): AsyncIterable<ChatStreamEvent> {
			calls += 1;
			if (calls === 1) {
				yield { type: 'tool_call', call: { id: 't1', args: { method: 'GET', path: `tasks/${taskId}` } } };
			} else {
				yield { type: 'text', delta: 'Looks good, no blockers.' };
			}
		};

		const result = await runPersonaAgent({
			ctx,
			router,
			persona,
			taskId,
			systemPrompt: 'You are a careful reviewer.',
			grantedPermissions: ['tasks:read', 'comments:write'],
			chat,
		});

		expect(result).toEqual({ text: 'Looks good, no blockers.', toolCalls: 1 });

		const [run] = await ctx.db.select<any>(`SELECT * FROM agent_work_runs WHERE task_id = ?`, [taskId]);
		expect(run.agent).toBe('local-llm');
		expect(run.actor_kind).toBe('persona');
		expect(run.actor_id).toBe('p-1');
		expect(run.status).toBe('succeeded');
	});

	it('stops after maxToolCalls even if the model keeps calling tools', async () => {
		const chat = async function* (): AsyncIterable<ChatStreamEvent> {
			yield { type: 'tool_call', call: { id: 't', args: { method: 'GET', path: `tasks/${taskId}` } } };
		};

		const result = await runPersonaAgent({
			ctx,
			router,
			persona,
			taskId,
			systemPrompt: 'loop forever',
			grantedPermissions: ['tasks:read'],
			chat,
			maxToolCalls: 3,
		});

		expect(result.toolCalls).toBe(3);
		expect(result.text).toBe('');
	});

	it('refuses to record a run when the persona lacks agent_work:write', async () => {
		await enableLocalPersona(ctx, 'p-1', ['tasks:read']);
		const chat = async function* (): AsyncIterable<ChatStreamEvent> {
			yield { type: 'text', delta: 'hi' };
		};
		await expect(
			runPersonaAgent({ ctx, router, persona, taskId, systemPrompt: 'x', grantedPermissions: ['tasks:read'], chat }),
		).rejects.toThrow('agent-work run');
	});
});
