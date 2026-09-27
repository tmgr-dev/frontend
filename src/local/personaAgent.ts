import { dispatchLocal } from './dispatch';
import type { LocalRouter } from './router';
import type { LocalContext } from './types';

export interface ChatMessage {
	role: 'system' | 'user' | 'assistant' | 'tool';
	content: string;
	tool_call_id?: string;
}

export interface ToolCall {
	id: string;
	args: { method: string; path: string; body?: unknown };
}

export type ChatStreamEvent = { type: 'text'; delta: string } | { type: 'tool_call'; call: ToolCall };

/** The Rust side turns the SSE stream from `llm_chat` into this; a test can fake it directly. */
export type ChatFn = (messages: ChatMessage[]) => AsyncIterable<ChatStreamEvent>;

const TOOL_NAME = 'tmgr_request';

/** One generic REST tool, exactly like the cloud `tmgr_request` MCP tool for personas: every call
 * still goes through `dispatchLocal`, so the whitelist gate (personaGate.ts) is what actually
 * decides what the persona may do — this loop never widens or narrows it. */
export const personaToolDefinition = (grantedPermissions: string[]) => ({
	name: TOOL_NAME,
	description:
		`Call the local TMGR REST API for this workspace. Restricted to this persona's granted ` +
		`permissions: ${grantedPermissions.join(', ') || '(none)'}.`,
	parameters: {
		type: 'object',
		properties: {
			method: { type: 'string', enum: ['GET', 'POST', 'PATCH', 'DELETE'] },
			path: { type: 'string' },
			body: { type: 'object' },
		},
		required: ['method', 'path'],
	},
});

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

/**
 * Runs one agent turn: system prompt + granted-permission tool, executing every tool call as the
 * persona actor via the local router, and recording a local agent-work run for it. Stops as soon as
 * the model answers with plain text, or after `maxToolCalls`.
 */
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
			let sawToolCall = false;
			let textBuffer = '';
			for await (const event of chat(messages)) {
				if (event.type === 'text') {
					textBuffer += event.delta;
					continue;
				}
				sawToolCall = true;
				toolCalls += 1;
				const { method, path, body } = event.call.args;
				const result = await dispatchLocal(router, personaCtx, method, path, body);
				messages.push({
					role: 'tool',
					tool_call_id: event.call.id,
					content: JSON.stringify(result ? result.data : { message: `no local route for ${method} ${path}` }),
				});
				if (toolCalls >= maxToolCalls) break;
			}
			if (!sawToolCall) {
				finalText = textBuffer;
				break;
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

/** "Ask persona" on a task: runs one turn, then posts whatever text it ends on as a comment
 * authored by the persona (needs `comments:write` in its grant, same as any other persona write). */
export const askPersonaOnTask = async (params: PersonaAgentParams): Promise<PersonaAgentResult> => {
	const result = await runPersonaAgent(params);
	if (result.text.trim()) {
		const actor = { kind: 'persona' as const, id: params.persona.uuid, name: params.persona.name };
		await dispatchLocal(params.router, { ...params.ctx, actor }, 'POST', `tasks/${params.taskId}/comments`, {
			message: result.text,
		});
	}
	return result;
};
