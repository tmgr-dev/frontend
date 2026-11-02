import { createLocalApi } from '../api';
import { dispatchLocal } from '../dispatch';
import type { ChatMessage, ChatStreamEvent } from '../personaAgent';
import {
	askPersonaOnTask,
	personaToolDefinition,
	runPersonaAgent,
} from '../personaAgent';
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

	const toolCallEvent = (path: string, id = 't1'): ChatStreamEvent => ({
		type: 'tool_call',
		call: {
			id,
			name: 'tmgr_request',
			rawArguments: JSON.stringify({ method: 'GET', path }),
			args: { method: 'GET', path },
		},
	});

	beforeEach(async () => {
		ctx = {
			db: memoryDb(),
			workspace: {
				id: -1,
				name: 'Personal',
				code: 'local-personal',
				schema_version: 0,
				created_at: '',
				path: '/tmp/x',
				database: '/tmp/x/workspace.db',
			},
			user: { id: 7, name: 'Yurij', email: 'me@example.com' },
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
		await enableLocalPersona(ctx, 'p-1', [
			'tasks:read',
			'comments:write',
			'agent_work:read',
			'agent_work:write',
		]);
		const created = await dispatchLocal(router, ctx, 'POST', 'tasks', {
			title: 'Review the PR',
		});
		taskId = created!.data.data.id;
	});

	it('still answers when agent work is turned off, without recording a run', async () => {
		await dispatchLocal(router, ctx, 'PUT', 'workspaces/-1/feature-toggles', {
			features: { agent_work: false },
		});
		const chat = async function* (): AsyncIterable<ChatStreamEvent> {
			yield { type: 'text', delta: 'Fine.' };
		};
		const result = await runPersonaAgent({
			ctx,
			router,
			persona,
			taskId,
			systemPrompt: 'Be careful.',
			grantedPermissions: ['tasks:read'],
			chat,
		});
		expect(result.text).toBe('Fine.');
		await dispatchLocal(router, ctx, 'PUT', 'workspaces/-1/feature-toggles', {
			features: { agent_work: true },
		});
		const overview = await dispatchLocal(
			router,
			ctx,
			'GET',
			`tasks/${taskId}/agent-work`,
		);
		expect(overview!.data.data.runs).toEqual([]);
	});

	it('calls one tool, then answers with text, and records the run for the persona', async () => {
		let calls = 0;
		const chat = async function* (): AsyncIterable<ChatStreamEvent> {
			calls += 1;
			if (calls === 1) {
				yield toolCallEvent(`tasks/${taskId}`);
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

		const [run] = await ctx.db.select<any>(
			`SELECT * FROM agent_work_runs WHERE task_id = ?`,
			[taskId],
		);
		expect(run.agent).toBe('local-llm');
		expect(run.actor_kind).toBe('persona');
		expect(run.actor_id).toBe('p-1');
		expect(run.status).toBe('succeeded');
	});

	it('pairs the assistant tool_calls message with a matching tool result message', async () => {
		const seenMessages: ChatMessage[][] = [];
		let calls = 0;
		const chat = async function* (
			messages: ChatMessage[],
		): AsyncIterable<ChatStreamEvent> {
			seenMessages.push(messages.map((m) => ({ ...m })));
			calls += 1;
			if (calls === 1) {
				yield toolCallEvent(`tasks/${taskId}`, 't1');
			} else {
				yield { type: 'text', delta: 'done' };
			}
		};

		await runPersonaAgent({
			ctx,
			router,
			persona,
			taskId,
			systemPrompt: 'x',
			grantedPermissions: ['tasks:read'],
			chat,
		});

		const secondCallMessages = seenMessages[1];
		const assistantMessage = secondCallMessages.find(
			(m) => m.role === 'assistant',
		);
		expect(assistantMessage?.tool_calls).toEqual([
			{
				id: 't1',
				type: 'function',
				function: {
					name: 'tmgr_request',
					arguments: JSON.stringify({ method: 'GET', path: `tasks/${taskId}` }),
				},
			},
		]);
		const toolMessage = secondCallMessages.find((m) => m.role === 'tool');
		expect(toolMessage?.tool_call_id).toBe('t1');
	});

	it('shapes the tool definition as an OpenAI function tool', () => {
		const tool = personaToolDefinition(['tasks:read']);
		expect(tool.type).toBe('function');
		expect(tool.function.name).toBe('tmgr_request');
		expect(tool.function.parameters.required).toEqual(['method', 'path']);
	});

	it('offers every method its route list names, PUT included', () => {
		const tool = personaToolDefinition(['comments:write', 'pages:write']);
		expect(tool.function.description).toContain('PUT comments/{id}');
		expect(tool.function.parameters.properties.method.enum).toEqual(
			expect.arrayContaining(['GET', 'POST', 'PATCH', 'DELETE', 'PUT']),
		);
	});

	it('runs a PUT tool call through the whitelist: own comment edited, another comment and ungranted routes refused', async () => {
		const own = await dispatchLocal(
			router,
			{ ...ctx, actor: { kind: 'persona', id: 'p-1', name: 'Reviewer' } },
			'POST',
			`tasks/${taskId}/comments`,
			{ message: 'mine' },
		);
		const others = await dispatchLocal(
			router,
			ctx,
			'POST',
			`tasks/${taskId}/comments`,
			{ message: 'the owner' },
		);
		const put = (path: string, body: unknown, id: string): ChatStreamEvent => ({
			type: 'tool_call',
			call: {
				id,
				name: 'tmgr_request',
				rawArguments: JSON.stringify({ method: 'PUT', path, body }),
				args: { method: 'PUT', path, body },
			},
		});
		const toolResults: any[] = [];
		let turn = 0;
		const chat = async function* (
			messages: ChatMessage[],
		): AsyncIterable<ChatStreamEvent> {
			turn += 1;
			if (turn === 1) {
				yield put(`comments/${own!.data.data.id}`, { message: 'edited' }, 'a');
				yield put(
					`comments/${others!.data.data.id}`,
					{ message: 'hijacked' },
					'b',
				);
				yield put(`tasks/${taskId}`, { title: 'renamed' }, 'c');
			} else {
				for (const m of messages)
					if (m.role === 'tool') toolResults.push(JSON.parse(m.content!));
				yield { type: 'text', delta: 'ok' };
			}
		};
		await runPersonaAgent({
			ctx,
			router,
			persona,
			taskId,
			systemPrompt: 'x',
			grantedPermissions: ['comments:write'],
			chat,
		});
		expect(toolResults[0].data.message).toBe('edited');
		expect(toolResults[1].code).toBe('NOT_OWN');
		expect(toolResults[2].code).toBeDefined();
		const untouched = await dispatchLocal(
			router,
			ctx,
			'GET',
			`tasks/${taskId}`,
		);
		expect(untouched!.data.data.title).toBe('Review the PR');
	});

	it('reports a malformed tool call instead of crashing, and keeps going', async () => {
		let calls = 0;
		const chat = async function* (): AsyncIterable<ChatStreamEvent> {
			calls += 1;
			if (calls === 1) {
				yield {
					type: 'tool_call',
					call: {
						id: 'bad',
						name: 'tmgr_request',
						rawArguments: '{}',
						args: {} as any,
					},
				};
			} else {
				yield { type: 'text', delta: 'done' };
			}
		};

		const result = await runPersonaAgent({
			ctx,
			router,
			persona,
			taskId,
			systemPrompt: 'x',
			grantedPermissions: ['tasks:read'],
			chat,
		});

		expect(result).toEqual({ text: 'done', toolCalls: 1 });
	});

	it('stops after maxToolCalls even if the model keeps calling tools', async () => {
		const chat = async function* (): AsyncIterable<ChatStreamEvent> {
			yield toolCallEvent(`tasks/${taskId}`, 't');
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

	it('posts the final text as a comment authored by the persona (ask persona on a task)', async () => {
		const chat = async function* (): AsyncIterable<ChatStreamEvent> {
			yield { type: 'text', delta: 'No blockers found.' };
		};
		await askPersonaOnTask({
			ctx,
			router,
			persona,
			taskId,
			systemPrompt: 'Review the task',
			grantedPermissions: ['comments:write'],
			chat,
		});
		const comments = await dispatchLocal(
			router,
			ctx,
			'GET',
			`tasks/${taskId}/comments`,
		);
		expect(comments!.data.data).toHaveLength(1);
		expect(comments!.data.data[0]).toMatchObject({
			message: 'No blockers found.',
			author: { kind: 'persona', id: 'p-1', name: 'Reviewer' },
		});
	});

	it('reports the text so far to onText as it arrives, restarting each model turn', async () => {
		let calls = 0;
		const chat = async function* (): AsyncIterable<ChatStreamEvent> {
			calls += 1;
			if (calls === 1) {
				yield { type: 'text', delta: 'Checking' };
				yield toolCallEvent(`tasks/${taskId}`);
			} else {
				yield { type: 'text', delta: 'All ' };
				yield { type: 'text', delta: 'good.' };
			}
		};
		const seen: string[] = [];
		const result = await askPersonaOnTask({
			ctx,
			router,
			persona,
			taskId,
			systemPrompt: 'x',
			grantedPermissions: ['tasks:read', 'comments:write'],
			chat,
			onText: (text) => seen.push(text),
		});
		expect(seen).toEqual(['Checking', 'All ', 'All good.']);
		expect(result.text).toBe('All good.');
		const comments = await dispatchLocal(
			router,
			ctx,
			'GET',
			`tasks/${taskId}/comments`,
		);
		expect(comments!.data.data[0].message).toBe('All good.');
	});

	it('throws instead of silently dropping the answer when posting the comment is refused', async () => {
		const chat = async function* (): AsyncIterable<ChatStreamEvent> {
			yield { type: 'text', delta: 'No blockers found.' };
		};
		await enableLocalPersona(ctx, 'p-1', [
			'agent_work:read',
			'agent_work:write',
		]);
		await expect(
			askPersonaOnTask({
				ctx,
				router,
				persona,
				taskId,
				systemPrompt: 'Review the task',
				grantedPermissions: [],
				chat,
			}),
		).rejects.toThrow('Could not post');
	});

	it('refuses to record a run when the persona lacks agent_work:write', async () => {
		await enableLocalPersona(ctx, 'p-1', ['tasks:read']);
		const chat = async function* (): AsyncIterable<ChatStreamEvent> {
			yield { type: 'text', delta: 'hi' };
		};
		await expect(
			runPersonaAgent({
				ctx,
				router,
				persona,
				taskId,
				systemPrompt: 'x',
				grantedPermissions: ['tasks:read'],
				chat,
			}),
		).rejects.toThrow('agent-work run');
	});
});
