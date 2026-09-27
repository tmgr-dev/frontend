import { dispatchLocal } from './dispatch';
import { PERSONA_WHITELIST } from './personaGate';
import type { LocalRouter } from './router';
import type { LocalContext } from './types';

export interface ChatMessage {
	role: 'system' | 'user' | 'assistant' | 'tool';
	content: string | null;
	tool_call_id?: string;
	tool_calls?: { id: string; type: 'function'; function: { name: string; arguments: string } }[];
}

export interface ToolCall {
	id: string;
	name: string;
	/** Echoed back verbatim in the assistant message the next turn sees. */
	rawArguments: string;
	args: { method: string; path: string; body?: unknown };
}

export type ChatStreamEvent = { type: 'text'; delta: string } | { type: 'tool_call'; call: ToolCall };

/** The Rust side turns the SSE stream from `llm_chat` into this; a test can fake it directly. */
export type ChatFn = (messages: ChatMessage[]) => AsyncIterable<ChatStreamEvent>;

const TOOL_NAME = 'tmgr_request';

/** Every call still goes through `dispatchLocal`, so the whitelist gate decides what runs, not this. */
export const personaToolDefinition = (grantedPermissions: string[]) => {
	const routes = PERSONA_WHITELIST.filter((entry) => grantedPermissions.includes(entry.permission)).map(
		(entry) => `${entry.method} ${entry.pattern.replace(/:(\w+)(\([^)]*\))?/g, '{$1}')}`,
	);
	return {
		type: 'function',
		function: {
			name: TOOL_NAME,
			description:
				`Call the local TMGR REST API for this workspace. Only these routes will succeed: ` +
				`${routes.join(', ') || '(none granted)'}.`,
			parameters: {
				type: 'object',
				properties: {
					method: { type: 'string', enum: ['GET', 'POST', 'PATCH', 'DELETE'] },
					path: { type: 'string' },
					body: { type: 'object' },
				},
				required: ['method', 'path'],
			},
		},
	};
};

export interface PersonaAgentParams {
	ctx: LocalContext;
	router: LocalRouter;
	persona: { uuid: string; name: string };
	taskId: number;
	systemPrompt: string;
	grantedPermissions: string[];
	chat: ChatFn;
	maxToolCalls?: number;
}

export interface PersonaAgentResult {
	text: string;
	toolCalls: number;
}

export const runPersonaAgent = async ({
	ctx,
	router,
	persona,
	taskId,
	systemPrompt,
	chat,
	maxToolCalls = 8,
}: PersonaAgentParams): Promise<PersonaAgentResult> => {
	const actor = { kind: 'persona' as const, id: persona.uuid, name: persona.name };
	const personaCtx: LocalContext = { ...ctx, actor };

	const started = await dispatchLocal(router, personaCtx, 'POST', `tasks/${taskId}/agent-work`, {
		agent: 'local-llm',
	});
	if (!started || started.status >= 400) {
		throw new Error(`Could not start a local agent-work run: ${started?.data?.message ?? 'no route'}`);
	}
	const runId = started.data.data.id;

	const messages: ChatMessage[] = [
		{ role: 'system', content: systemPrompt },
		{ role: 'user', content: `Answer about task #${taskId}.` },
	];

	let toolCalls = 0;
	let finalText = '';
	let status: 'succeeded' | 'failed' = 'succeeded';
	try {
		while (toolCalls < maxToolCalls) {
			const calls: ToolCall[] = [];
			let textBuffer = '';
			for await (const event of chat(messages)) {
				if (event.type === 'text') textBuffer += event.delta;
				else calls.push(event.call);
			}
			if (!calls.length) {
				finalText = textBuffer;
				break;
			}
			messages.push({
				role: 'assistant',
				content: textBuffer || null,
				tool_calls: calls.map((call) => ({
					id: call.id,
					type: 'function',
					function: { name: call.name, arguments: call.rawArguments },
				})),
			});
			for (const call of calls) {
				if (toolCalls >= maxToolCalls) break;
				toolCalls += 1;
				const { method, path, body } = call.args ?? {};
				const content =
					typeof method !== 'string' || typeof path !== 'string'
						? { message: 'method and path are required strings' }
						: (await dispatchLocal(router, personaCtx, method, path, body))?.data ??
							{ message: `no local route for ${method} ${path}` };
				messages.push({ role: 'tool', tool_call_id: call.id, content: JSON.stringify(content) });
			}
		}
	} catch (error) {
		status = 'failed';
		throw error;
	} finally {
		await dispatchLocal(router, personaCtx, 'POST', `agent-work/${runId}/finish`, {
			status,
			summary: finalText.slice(0, 2000),
		});
	}
	return { text: finalText, toolCalls };
};

/** Needs `comments:write` in the grant, same as any other persona-authored comment. */
export const askPersonaOnTask = async (params: PersonaAgentParams): Promise<PersonaAgentResult> => {
	const result = await runPersonaAgent(params);
	if (result.text.trim()) {
		const actor = { kind: 'persona' as const, id: params.persona.uuid, name: params.persona.name };
		const posted = await dispatchLocal(
			params.router,
			{ ...params.ctx, actor },
			'POST',
			`tasks/${params.taskId}/comments`,
			{ message: result.text },
		);
		if (!posted || posted.status >= 400) {
			throw new Error(`Could not post the persona's answer as a comment: ${posted?.data?.message ?? 'no route'}`);
		}
	}
	return result;
};
